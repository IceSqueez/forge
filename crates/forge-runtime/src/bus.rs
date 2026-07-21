use crate::buf::RingBuffer;
use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventsError};
use forge_storage::{EventLogRepo, StorageError};
use forge_types::EventId;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use time::OffsetDateTime;
use tokio::sync::{Notify, broadcast};

const CHANNEL_CAP: usize = 1_024;
const RING_CAP: usize = 10_000;

#[derive(Debug, thiserror::Error)]
pub enum BusError {
    #[error("event {0} not found in ring or persistent log")]
    EventNotFound(EventId),
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
}

pub struct NullEventLogRepo;

#[async_trait]
impl EventLogRepo for NullEventLogRepo {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

pub struct EventBus {
    sender: broadcast::Sender<Event>,
    ring: Mutex<RingBuffer<Event>>,
    total_published: AtomicU64,
    event_log: Arc<dyn EventLogRepo>,
    flush_shutdown: Arc<Notify>,
}

pub struct EventSubscription(broadcast::Receiver<Event>);

impl EventSubscription {
    pub async fn recv(&mut self) -> Result<Event, EventsError> {
        match self.0.recv().await {
            Ok(event) => Ok(event),
            Err(broadcast::error::RecvError::Closed) => Err(EventsError::BusClosed),
            Err(broadcast::error::RecvError::Lagged(_)) => Err(EventsError::LaggingReceiver),
        }
    }

    /// `Ok(None)` signals the channel is momentarily empty; the caller stops draining.
    pub fn try_recv(&mut self) -> Result<Option<Event>, EventsError> {
        match self.0.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(broadcast::error::TryRecvError::Closed) => Err(EventsError::BusClosed),
            Err(broadcast::error::TryRecvError::Lagged(_)) => Err(EventsError::LaggingReceiver),
        }
    }

    pub(crate) fn into_receiver(self) -> broadcast::Receiver<Event> {
        self.0
    }
}

impl EventBus {
    pub fn new(event_log: Arc<dyn EventLogRepo>) -> Arc<Self> {
        Self::with_caps(event_log, CHANNEL_CAP, RING_CAP)
    }

    pub(crate) fn with_caps(
        event_log: Arc<dyn EventLogRepo>,
        channel_cap: usize,
        ring_cap: usize,
    ) -> Arc<Self> {
        let (sender, _) = broadcast::channel(channel_cap);
        Arc::new(Self {
            sender,
            ring: Mutex::new(RingBuffer::new(ring_cap)),
            total_published: AtomicU64::new(0),
            event_log,
            flush_shutdown: Arc::new(Notify::new()),
        })
    }

    /// Slow subscribers lag (broadcast semantics); publisher never blocks on them.
    pub fn publish(&self, event: Event) {
        let _ = self.sender.send(event.clone());
        self.ring
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(event);
        self.total_published.fetch_add(1, Ordering::Relaxed);
    }

    /// Stores an event into the retained ring for later `replay_and_publish`
    /// and observability, WITHOUT broadcasting it to subscribers. No subscriber
    /// (including the trigger pipeline) observes a recorded event, so recording
    /// then replaying evaluates the event exactly once instead of twice.
    pub fn record(&self, event: Event) {
        self.ring
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(event);
    }

    pub fn subscribe(&self) -> EventSubscription {
        EventSubscription(self.sender.subscribe())
    }

    /// Returns `None` when `event_id` is not in the retained ring.
    pub fn lookup(&self, event_id: EventId) -> Option<Event> {
        self.ring
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|e| e.id == event_id)
            .cloned()
    }

    pub fn recent(&self, limit: usize) -> Vec<Event> {
        self.ring
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Returns up to `limit` events in newest-first order.
    ///
    /// When `since` is `None` this is equivalent to `recent(limit)`.
    /// When `since` is `Some(id)` only events published after (exclusive) that
    /// event are returned.  The ring is checked first; if the anchor id has been
    /// evicted the call falls back to `EventLogRepo::recent_since`.  If the
    /// anchor is absent from both ring and log the result is an empty `Vec`.
    ///
    /// The ring lock is never held across the async DB fallback.
    pub async fn recent_since(&self, limit: usize, since: Option<EventId>) -> Vec<Event> {
        let since_id = match since {
            None => return self.recent(limit),
            Some(id) => id,
        };

        let ring_result = {
            let guard = self.ring.lock().unwrap_or_else(|p| p.into_inner());
            let items: Vec<&Event> = guard.iter().collect();
            items.iter().position(|e| e.id == since_id).map(|pos| {
                items[pos + 1..]
                    .iter()
                    .rev()
                    .take(limit)
                    .map(|e| (*e).clone())
                    .collect::<Vec<Event>>()
            })
        };

        if let Some(events) = ring_result {
            return events;
        }

        self.event_log
            .recent_since(limit, Some(since_id))
            .await
            .unwrap_or_default()
    }

    /// Loads the original event from the ring (fast path) or the persistent event log (fallback),
    /// stamps a fresh `EventId`, sets `replay: true`, and publishes through the full bus pipeline.
    ///
    /// Downstream triggers fire exactly as they would for the original event. The replayed event
    /// carries `caused_by` from the original, preserving causation chain navigability.
    pub async fn replay_and_publish(&self, event_id: EventId) -> Result<(), BusError> {
        let original = match self.lookup(event_id) {
            Some(e) => e,
            None => self
                .event_log
                .get(event_id)
                .await
                .map_err(BusError::Storage)?
                .ok_or(BusError::EventNotFound(event_id))?,
        };

        let replayed = Event {
            id: EventId::new(),
            source: original.source,
            kind: original.kind.clone(),
            timestamp: OffsetDateTime::now_utc(),
            payload: original.payload.clone(),
            caused_by: original.caused_by,
            replay: true,
        };

        self.publish(replayed);
        Ok(())
    }

    /// Subscribes a dedicated persistence receiver and spawns the flush task.
    ///
    /// The subscription is created synchronously before the task is spawned, so events
    /// published immediately after this call are guaranteed to be received by the task.
    /// Callers that do not need persistence (tests using `NullEventLogRepo`) may skip
    /// this call entirely.
    pub fn spawn_flush_task(bus: Arc<Self>) {
        let recv = bus.sender.subscribe();
        let repo = Arc::clone(&bus.event_log);
        let shutdown = Arc::clone(&bus.flush_shutdown);
        tokio::spawn(event_log_flush_task(recv, repo, shutdown));
    }

    /// Signals the flush task to drain remaining events and exit.
    ///
    /// Uses `notify_one` (not `notify_waiters`) so the permit is stored even if the
    /// flush task has not yet polled its shutdown future.
    pub fn shutdown(&self) {
        self.flush_shutdown.notify_one();
    }
}

async fn event_log_flush_task(
    mut recv: broadcast::Receiver<Event>,
    repo: Arc<dyn EventLogRepo>,
    shutdown: Arc<Notify>,
) {
    loop {
        tokio::select! {
            biased;
            _ = shutdown.notified() => {
                while let Ok(ev) = recv.try_recv() {
                    if let Err(e) = repo.insert(&ev).await {
                        // Event remains in ring until evicted; persistence is lossy on error - no retry.
                        tracing::warn!(error = %e, "event_log drain insert failed");
                    }
                }
                return;
            }
            result = recv.recv() => {
                match result {
                    Ok(ev) => {
                        if let Err(e) = repo.insert(&ev).await {
                            // Event remains in ring until evicted; persistence is lossy on error - no retry.
                            tracing::warn!(error = %e, "event_log insert failed; event not persisted");
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            skipped = n,
                            "event_log flush task lagged; events may not be persisted"
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        }
    }
}

impl EventPublisher for EventBus {
    fn publish(&self, event: Event) {
        EventBus::publish(self, event);
    }
}

const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    let _ = assert_send_sync::<EventBus>;
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use std::sync::Arc;
    use std::time::Duration;

    fn null_bus() -> Arc<EventBus> {
        EventBus::new(Arc::new(NullEventLogRepo))
    }

    fn core_event(kind: &str) -> Event {
        Event::new(EventSource::Core, kind, serde_json::Value::Null)
    }

    struct BackedEventLog(Arc<SqliteBackend>);

    #[async_trait]
    impl EventLogRepo for BackedEventLog {
        async fn insert(&self, event: &Event) -> Result<(), StorageError> {
            self.0.event_log_repo().insert(event).await
        }

        async fn get(&self, id: EventId) -> Result<Option<Event>, StorageError> {
            self.0.event_log_repo().get(id).await
        }

        async fn recent(&self, limit: usize) -> Result<Vec<Event>, StorageError> {
            self.0.event_log_repo().recent(limit).await
        }

        async fn recent_since(
            &self,
            limit: usize,
            since: Option<EventId>,
        ) -> Result<Vec<Event>, StorageError> {
            self.0.event_log_repo().recent_since(limit, since).await
        }

        async fn prune_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
            self.0.event_log_repo().prune_before(cutoff).await
        }
    }

    async fn backed_bus_with_caps(
        channel_cap: usize,
        ring_cap: usize,
    ) -> (Arc<EventBus>, Arc<SqliteBackend>) {
        let backend = Arc::new(
            SqliteBackend::open_with_key(":memory:", [0xab; 32])
                .await
                .unwrap(),
        );
        let event_log: Arc<dyn EventLogRepo> = Arc::new(BackedEventLog(Arc::clone(&backend)));
        let bus = EventBus::with_caps(event_log, channel_cap, ring_cap);
        (bus, backend)
    }

    #[tokio::test]
    async fn publish_subscribe_roundtrip() {
        let bus = null_bus();
        let mut sub = bus.subscribe();
        let ev = core_event("action.start");
        let expected_id = ev.id;
        bus.publish(ev);
        let received = sub.recv().await.unwrap();
        assert_eq!(received.id, expected_id);
    }

    #[tokio::test]
    async fn multiple_subscribers_each_receive_same_event() {
        let bus = null_bus();
        let mut sub_a = bus.subscribe();
        let mut sub_b = bus.subscribe();
        let ev = core_event("queue.paused");
        let expected_id = ev.id;
        bus.publish(ev);
        assert_eq!(sub_a.recv().await.unwrap().id, expected_id);
        assert_eq!(sub_b.recv().await.unwrap().id, expected_id);
    }

    #[test]
    fn ring_buffer_fills_and_evicts_oldest() {
        let bus = null_bus();
        for i in 0..RING_CAP + 5 {
            bus.publish(core_event(&format!("tick.{i}")));
        }
        assert_eq!(bus.recent(RING_CAP + 5).len(), RING_CAP);
    }

    #[test]
    fn lookup_returns_stored_event_by_id() {
        let bus = null_bus();
        let ev = core_event("action.done");
        let id = ev.id;
        bus.publish(ev);
        let found = bus.lookup(id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, id);
    }

    #[test]
    fn lookup_returns_none_for_missing_id() {
        let bus = null_bus();
        let ghost_id = EventId::new();
        assert!(bus.lookup(ghost_id).is_none());
    }

    #[test]
    fn record_does_not_broadcast_to_subscribers() {
        // Crux of the double-fire fix: `record` must store-only. A subscriber
        // present before the call must observe nothing. If `record` reverts to
        // `publish`, try_recv yields the event instead of Empty.
        let bus = null_bus();
        let mut rx = bus.subscribe().into_receiver();
        bus.record(core_event("silent.record"));
        assert!(matches!(
            rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn record_retains_event_in_ring_for_replay() {
        // `record` is store-only, not a no-op: the event stays in the ring so a
        // later replay_and_publish / lookup can find it.
        let bus = null_bus();
        let ev = core_event("recorded.candidate");
        let id = ev.id;
        bus.record(ev);
        assert_eq!(bus.lookup(id).map(|e| e.id), Some(id));
    }

    #[tokio::test]
    async fn record_then_replay_delivers_event_exactly_once() {
        // Regression guard for the test-run double-fire: record (store-only) plus
        // replay_and_publish must reach a subscriber exactly ONCE - the single
        // replayed broadcast. A revert of `record` to `publish` delivers twice.
        let bus = null_bus();
        let mut rx = bus.subscribe().into_receiver();

        let ev = core_event("trigger.candidate");
        let id = ev.id;
        bus.record(ev);
        bus.replay_and_publish(id).await.unwrap();

        let first = rx.try_recv().unwrap();
        assert!(
            first.replay,
            "the single delivery must be the replayed event"
        );
        assert_eq!(first.kind, "trigger.candidate");
        assert!(
            matches!(rx.try_recv(), Err(broadcast::error::TryRecvError::Empty)),
            "record must not broadcast; only replay delivers, so exactly one event arrives"
        );
    }

    #[tokio::test]
    async fn lagged_subscriber_gets_lagging_error() {
        let bus = null_bus();
        let mut slow = bus.subscribe();
        for i in 0..CHANNEL_CAP + 10 {
            bus.publish(core_event(&format!("flood.{i}")));
        }
        let mut got_lagged = false;
        loop {
            match slow.recv().await {
                Ok(_) => {}
                Err(EventsError::LaggingReceiver) => {
                    got_lagged = true;
                    break;
                }
                Err(EventsError::BusClosed) | Err(_) => break,
            }
        }
        assert!(
            got_lagged,
            "slow subscriber must receive LaggingReceiver error"
        );
    }

    #[test]
    fn recent_returns_newest_first_up_to_limit() {
        let bus = null_bus();
        let mut ids = Vec::new();
        for i in 0..5 {
            let ev = core_event(&format!("ev.{i}"));
            ids.push(ev.id);
            bus.publish(ev);
        }
        let recent = bus.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].id, ids[4]);
        assert_eq!(recent[1].id, ids[3]);
        assert_eq!(recent[2].id, ids[2]);
    }

    #[tokio::test]
    async fn replay_and_publish_ring_hit() {
        let bus = null_bus();
        let mut sub = bus.subscribe();

        let original = core_event("action.start");
        let original_id = original.id;
        bus.publish(original);
        let _ = sub.recv().await.unwrap();

        bus.replay_and_publish(original_id).await.unwrap();
        let replayed = sub.recv().await.unwrap();

        assert!(replayed.replay, "replayed event must have replay=true");
        assert_ne!(
            replayed.id, original_id,
            "replayed event must have a fresh id"
        );
        assert_eq!(replayed.kind, "action.start");
    }

    #[tokio::test]
    async fn replay_and_publish_db_fallback() {
        let (bus, backend) = backed_bus_with_caps(64, 2).await;
        let mut sub = bus.subscribe();

        let ev1 = core_event("ev.first");
        let ev1_id = ev1.id;
        backend.event_log_repo().insert(&ev1).await.unwrap();
        bus.publish(ev1);
        let _ = sub.recv().await.unwrap();

        let ev2 = core_event("ev.second");
        backend.event_log_repo().insert(&ev2).await.unwrap();
        bus.publish(ev2);
        let _ = sub.recv().await.unwrap();

        let ev3 = core_event("ev.third");
        backend.event_log_repo().insert(&ev3).await.unwrap();
        bus.publish(ev3);
        let _ = sub.recv().await.unwrap();

        assert!(
            bus.lookup(ev1_id).is_none(),
            "ev1 must be evicted from the 2-slot ring"
        );

        bus.replay_and_publish(ev1_id).await.unwrap();
        let replayed = sub.recv().await.unwrap();

        assert!(replayed.replay);
        assert_ne!(replayed.id, ev1_id);
        assert_eq!(replayed.kind, "ev.first");
    }

    #[tokio::test]
    async fn replay_and_publish_not_found_returns_error() {
        let bus = null_bus();
        let ghost_id = EventId::new();
        let result = bus.replay_and_publish(ghost_id).await;
        assert!(
            matches!(result, Err(BusError::EventNotFound(id)) if id == ghost_id),
            "must return EventNotFound for unknown id"
        );
    }

    struct FailFirstRepo {
        call_count: AtomicU64,
        stored: Mutex<Vec<Event>>,
    }

    impl FailFirstRepo {
        fn new() -> Self {
            Self {
                call_count: AtomicU64::new(0),
                stored: Mutex::new(Vec::new()),
            }
        }

        fn stored_count(&self) -> usize {
            self.stored.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl EventLogRepo for FailFirstRepo {
        async fn insert(&self, event: &Event) -> Result<(), StorageError> {
            let n = self.call_count.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                return Err(StorageError::NotReady);
            }
            self.stored.lock().unwrap().push(event.clone());
            Ok(())
        }

        async fn get(&self, _id: EventId) -> Result<Option<Event>, StorageError> {
            Ok(None)
        }

        async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
            Ok(self.stored.lock().unwrap().clone())
        }

        async fn recent_since(
            &self,
            limit: usize,
            _since: Option<EventId>,
        ) -> Result<Vec<Event>, StorageError> {
            Ok(self
                .stored
                .lock()
                .unwrap()
                .iter()
                .rev()
                .take(limit)
                .cloned()
                .collect())
        }

        async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn flush_task_persists_published_events() {
        let (bus, backend) = backed_bus_with_caps(CHANNEL_CAP, RING_CAP).await;
        EventBus::spawn_flush_task(Arc::clone(&bus));

        let ev1 = core_event("flush.a");
        let ev2 = core_event("flush.b");
        let ev3 = core_event("flush.c");
        let ids = [ev1.id, ev2.id, ev3.id];

        bus.publish(ev1);
        bus.publish(ev2);
        bus.publish(ev3);

        tokio::time::sleep(Duration::from_millis(100)).await;

        let persisted = backend.event_log_repo().recent(100).await.unwrap();
        let persisted_ids: Vec<_> = persisted.iter().map(|e| e.id).collect();
        for id in ids {
            assert!(
                persisted_ids.contains(&id),
                "event {id} not found in persisted log"
            );
        }
    }

    #[tokio::test]
    async fn flush_task_continues_after_insert_error() {
        let repo: Arc<FailFirstRepo> = Arc::new(FailFirstRepo::new());
        let bus = EventBus::with_caps(
            Arc::clone(&repo) as Arc<dyn EventLogRepo>,
            CHANNEL_CAP,
            RING_CAP,
        );
        EventBus::spawn_flush_task(Arc::clone(&bus));

        bus.publish(core_event("err.first"));
        bus.publish(core_event("err.second"));
        bus.publish(core_event("err.third"));

        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(
            repo.stored_count(),
            2,
            "first insert must be rejected, subsequent two must succeed"
        );
    }

    #[tokio::test]
    async fn flush_task_shutdown_drains_pending_events() {
        let (bus, backend) = backed_bus_with_caps(CHANNEL_CAP, RING_CAP).await;
        EventBus::spawn_flush_task(Arc::clone(&bus));

        let ev1 = core_event("drain.a");
        let ev2 = core_event("drain.b");
        let ev3 = core_event("drain.c");
        let ids = [ev1.id, ev2.id, ev3.id];

        bus.publish(ev1);
        bus.publish(ev2);
        bus.publish(ev3);
        bus.shutdown();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let persisted = backend.event_log_repo().recent(100).await.unwrap();
        let persisted_ids: Vec<_> = persisted.iter().map(|e| e.id).collect();
        for id in ids {
            assert!(
                persisted_ids.contains(&id),
                "event {id} not persisted after shutdown drain"
            );
        }
    }

    #[tokio::test]
    async fn recent_since_none_returns_newest_first() {
        let bus = null_bus();
        let mut ids = Vec::new();
        for i in 0..7 {
            let ev = core_event(&format!("ev.{i}"));
            ids.push(ev.id);
            bus.publish(ev);
        }
        let result = bus.recent_since(5, None).await;
        assert_eq!(result.len(), 5);
        assert_eq!(result[0].id, ids[6]);
        assert_eq!(result[1].id, ids[5]);
        assert_eq!(result[4].id, ids[2]);
    }

    #[tokio::test]
    async fn recent_since_anchor_in_ring_returns_newer_events() {
        let bus = null_bus();
        let mut ids = Vec::new();
        for i in 0..5 {
            let ev = core_event(&format!("ev.{i}"));
            ids.push(ev.id);
            bus.publish(ev);
        }
        let result = bus.recent_since(100, Some(ids[2])).await;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, ids[4]);
        assert_eq!(result[1].id, ids[3]);
    }

    #[tokio::test]
    async fn recent_since_anchor_at_end_returns_empty() {
        let bus = null_bus();
        let mut ids = Vec::new();
        for i in 0..3 {
            let ev = core_event(&format!("ev.{i}"));
            ids.push(ev.id);
            bus.publish(ev);
        }
        let result = bus.recent_since(100, Some(ids[2])).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn recent_since_limit_respected_within_ring() {
        let bus = null_bus();
        let mut ids = Vec::new();
        for i in 0..6 {
            let ev = core_event(&format!("ev.{i}"));
            ids.push(ev.id);
            bus.publish(ev);
        }
        let result = bus.recent_since(2, Some(ids[0])).await;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, ids[5]);
        assert_eq!(result[1].id, ids[4]);
    }

    #[tokio::test]
    async fn recent_since_evicted_anchor_falls_back_to_db() {
        let (bus, backend) = backed_bus_with_caps(64, 2).await;

        let mut ev1 = core_event("ev.first");
        ev1.timestamp = time::OffsetDateTime::from_unix_timestamp(1_000_000).unwrap();
        let mut ev2 = core_event("ev.second");
        ev2.timestamp = time::OffsetDateTime::from_unix_timestamp(1_000_001).unwrap();
        let mut ev3 = core_event("ev.third");
        ev3.timestamp = time::OffsetDateTime::from_unix_timestamp(1_000_002).unwrap();

        let ev1_id = ev1.id;
        let ev2_id = ev2.id;
        let ev3_id = ev3.id;

        backend.event_log_repo().insert(&ev1).await.unwrap();
        backend.event_log_repo().insert(&ev2).await.unwrap();
        backend.event_log_repo().insert(&ev3).await.unwrap();

        bus.publish(ev1);
        bus.publish(ev2);
        bus.publish(ev3);

        assert!(
            bus.lookup(ev1_id).is_none(),
            "ev1 must be evicted from the 2-slot ring"
        );

        let result = bus.recent_since(100, Some(ev1_id)).await;
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, ev3_id);
        assert_eq!(result[1].id, ev2_id);
    }
}
