use crate::config::Config;
use crate::delivery::{
    CriticalSink, CriticalSinks, CriticalSubscription, EVENT_LOG, LaneTable, UNNAMED_OBSERVER,
};
use crate::delivery_loss::{
    ConsumerLoss, ConsumerLossCounters, DeliveryTier, LossLedger, LossWatch,
};
use crate::event_log_writer::EventLogSink;
use crate::event_ring::EventRing;
use crate::persist_batch::{BatchPolicy, FlushTicket, run_batched};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use forge_events::{DeliveryLane, Event, EventPublisher, EventsError};
use forge_registry::TriggerRegistry;
use forge_storage::{EventLogRepo, StorageError};
use forge_types::EventId;
use futures_util::FutureExt;
use futures_util::future::Shared;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use time::OffsetDateTime;
use tokio::sync::{broadcast, oneshot, watch};

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
    sender: broadcast::Sender<Arc<Event>>,
    ring: Mutex<EventRing>,
    lanes: ArcSwap<LaneTable>,
    critical: CriticalSinks,
    loss: Arc<LossLedger>,
    priority_capacity: usize,
    bulk_capacity: usize,
    total_published: AtomicU64,
    event_log: Arc<dyn EventLogRepo>,
    batch_policy: BatchPolicy,
    run_history_capacity: usize,
    flush_stop: watch::Sender<bool>,
    flush_abandon: watch::Sender<bool>,
    abandoned_rows: Arc<AtomicU64>,
    /// One per persisting consumer; each resolves once that consumer has drained or given up.
    flushes: Mutex<Vec<Shared<oneshot::Receiver<()>>>>,
}

pub enum Delivery {
    Event(Arc<Event>),
    /// This observer fell behind by that many events; they are already counted in drop accounting.
    Skipped(u64),
    Closed,
}

pub struct EventSubscription {
    receiver: broadcast::Receiver<Arc<Event>>,
    loss: Arc<ConsumerLossCounters>,
}

impl EventSubscription {
    pub async fn next(&mut self) -> Delivery {
        match self.receiver.recv().await {
            Ok(event) => Delivery::Event(event),
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                self.loss.lagged(skipped);
                Delivery::Skipped(skipped)
            }
            Err(broadcast::error::RecvError::Closed) => Delivery::Closed,
        }
    }

    pub async fn recv(&mut self) -> Result<Event, EventsError> {
        match self.next().await {
            Delivery::Event(event) => Ok(Arc::unwrap_or_clone(event)),
            Delivery::Skipped(_) => Err(EventsError::LaggingReceiver),
            Delivery::Closed => Err(EventsError::BusClosed),
        }
    }

    /// `Ok(None)` signals the channel is momentarily empty; the caller stops draining.
    pub fn try_recv(&mut self) -> Result<Option<Event>, EventsError> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(Arc::unwrap_or_clone(event))),
            Err(broadcast::error::TryRecvError::Empty) => Ok(None),
            Err(broadcast::error::TryRecvError::Closed) => Err(EventsError::BusClosed),
            Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                self.loss.lagged(skipped);
                Err(EventsError::LaggingReceiver)
            }
        }
    }
}

impl EventBus {
    pub fn new(event_log: Arc<dyn EventLogRepo>) -> Arc<Self> {
        Self::with_config(event_log, &Config::default())
    }

    pub fn with_config(event_log: Arc<dyn EventLogRepo>, config: &Config) -> Arc<Self> {
        let (sender, _) = broadcast::channel(config.bus_observer_capacity.max(1));
        Arc::new(Self {
            sender,
            ring: Mutex::new(EventRing::new(config.bus_ring_retention)),
            lanes: ArcSwap::from_pointee(LaneTable::default()),
            critical: CriticalSinks::new(),
            loss: LossLedger::new(),
            priority_capacity: config.critical_priority_capacity,
            bulk_capacity: config.critical_bulk_capacity,
            total_published: AtomicU64::new(0),
            event_log,
            batch_policy: BatchPolicy::from_config(config),
            run_history_capacity: config.run_history_capacity.max(1),
            flush_stop: watch::channel(false).0,
            flush_abandon: watch::channel(false).0,
            abandoned_rows: Arc::new(AtomicU64::new(0)),
            flushes: Mutex::new(Vec::new()),
        })
    }

    /// Never blocks: a full critical lane or a lagging observer loses events to drop accounting, not the publisher's time.
    pub fn publish(&self, event: Event) {
        let event = Arc::new(event);
        self.ring
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(Arc::clone(&event));
        let lane = self.lanes.load().lane_of(&event);
        self.critical.deliver(&event, lane);
        let _ = self.sender.send(event);
        self.total_published.fetch_add(1, Ordering::Relaxed);
    }

    /// Stores into the ring WITHOUT broadcasting, so record-then-replay evaluates the event once, not twice.
    pub fn record(&self, event: Event) {
        self.ring
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(Arc::new(event));
    }

    /// Replaces any earlier declaration; until the first one, platform events ride the priority lane
    /// and only core run telemetry is bulk.
    pub fn declare_lanes(&self, registry: &TriggerRegistry) {
        self.lanes
            .store(Arc::new(LaneTable::from_registry(registry)));
    }

    pub fn lane_of(&self, event: &Event) -> DeliveryLane {
        self.lanes.load().lane_of(event)
    }

    pub fn subscribe(&self) -> EventSubscription {
        self.subscribe_observer(UNNAMED_OBSERVER)
    }

    /// Lossy tier: lag is counted under `consumer` in drop accounting.
    pub fn subscribe_observer(&self, consumer: &'static str) -> EventSubscription {
        EventSubscription {
            receiver: self.sender.subscribe(),
            loss: self.loss.counters(consumer, DeliveryTier::Observer),
        }
    }

    /// Lossless tier: a queue of this consumer's own, fed on every publish from now on.
    pub fn subscribe_critical(&self, consumer: &'static str) -> CriticalSubscription {
        let (sink, subscription) = CriticalSink::open(
            self.priority_capacity,
            self.bulk_capacity,
            self.loss.counters(consumer, DeliveryTier::Critical),
        );
        self.critical.add(sink);
        subscription
    }

    pub fn loss_report(&self) -> Vec<ConsumerLoss> {
        self.loss.report()
    }

    pub fn watch_loss(&self) -> LossWatch {
        self.loss.watch()
    }

    /// Returns `None` when `event_id` is not in the retained ring.
    pub fn lookup(&self, event_id: EventId) -> Option<Event> {
        self.ring
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(event_id)
            .map(|event| Event::clone(event))
    }

    pub(crate) fn count_in_lineage(
        &self,
        from: EventId,
        counts: impl Fn(&Event) -> bool,
        ceiling: usize,
    ) -> usize {
        let ring = self.ring.lock().unwrap_or_else(|p| p.into_inner());
        let mut matched = 0;
        let mut cursor = Some(from);
        while let Some(id) = cursor
            && matched < ceiling
        {
            let Some(event) = ring.get(id) else {
                break;
            };
            if counts(event) {
                matched += 1;
            }
            cursor = event.caused_by;
        }
        matched
    }

    pub fn recent(&self, limit: usize) -> Vec<Event> {
        self.ring
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .rev()
            .take(limit)
            .map(|event| Event::clone(event))
            .collect()
    }

    /// Falls back to `EventLogRepo::recent_since` if evicted; the ring lock is never held across that await.
    pub async fn recent_since(&self, limit: usize, since: Option<EventId>) -> Vec<Event> {
        let since_id = match since {
            None => return self.recent(limit),
            Some(id) => id,
        };

        let ring_result = {
            let ring = self.ring.lock().unwrap_or_else(|p| p.into_inner());
            ring.position(since_id).map(|pos| {
                ring.after(pos)
                    .rev()
                    .take(limit)
                    .map(|event| Event::clone(event))
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

    /// Stamps a fresh `EventId` and `replay: true` but keeps the original `caused_by` for causation navigability.
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
            kind: original.kind,
            timestamp: OffsetDateTime::now_utc(),
            payload: original.payload,
            caused_by: original.caused_by,
            replay: true,
        };

        self.publish(replayed);
        Ok(())
    }

    /// Subscribes before spawning the flush task, so events published immediately after never get missed.
    pub fn spawn_flush_task(bus: Arc<Self>) {
        let subscription = bus.subscribe_critical(EVENT_LOG);
        let sink = EventLogSink::new(
            Arc::clone(&bus.event_log),
            bus.loss.counters(EVENT_LOG, DeliveryTier::Critical),
        );
        tokio::spawn(run_batched(
            subscription,
            sink,
            bus.batch_policy,
            bus.flush_ticket(),
        ));
    }

    pub(crate) fn batch_policy(&self) -> BatchPolicy {
        self.batch_policy
    }

    pub(crate) fn run_history_capacity(&self) -> usize {
        self.run_history_capacity
    }

    pub(crate) fn loss_counters(
        &self,
        consumer: &'static str,
        tier: DeliveryTier,
    ) -> Arc<ConsumerLossCounters> {
        self.loss.counters(consumer, tier)
    }

    /// A consumer registered here is waited for by `await_flush`.
    pub(crate) fn flush_ticket(&self) -> FlushTicket {
        let (done, finished) = oneshot::channel();
        self.flushes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(finished.shared());
        FlushTicket::new(self.flush_stop.subscribe(), done).abandonable(
            self.flush_abandon.subscribe(),
            Arc::clone(&self.abandoned_rows),
        )
    }

    /// Latched: a persisting consumer that starts after this call drains and stops at once.
    pub fn shutdown(&self) {
        self.flush_stop.send_replace(true);
        for entry in self.loss.report() {
            if entry.loss.total() > 0 {
                tracing::warn!(
                    consumer = entry.consumer,
                    tier = ?entry.tier,
                    priority_dropped = entry.loss.priority_dropped,
                    bulk_dropped = entry.loss.bulk_dropped,
                    skipped = entry.loss.skipped,
                    unwritten = entry.loss.unwritten,
                    "event loss since start"
                );
            }
        }
    }

    /// Awaits every persisting consumer's shutdown drain; one that already exited counts as drained.
    pub async fn await_flush(&self) {
        let pending: Vec<Shared<oneshot::Receiver<()>>> = self
            .flushes
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        futures_util::future::join_all(pending).await;
    }

    /// Consumers still draining stop committing and count what they held as unwritten; resolves
    /// once they all have. Returns the rows given up since start.
    pub async fn abandon_flush(&self) -> u64 {
        self.flush_abandon.send_replace(true);
        self.await_flush().await;
        self.abandoned_rows.load(Ordering::Relaxed)
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
    use crate::test_support::{Sandboxed, sandboxed_backend};
    use forge_events::EventSource;
    use forge_storage::DataProvider;
    use forge_storage_sqlite::SqliteBackend;
    use std::sync::Arc;
    use std::time::Duration;

    const CHANNEL_CAP: usize = 16;
    const RING_CAP: usize = 64;

    fn null_bus() -> Arc<EventBus> {
        bus_with_caps(Arc::new(NullEventLogRepo), CHANNEL_CAP, RING_CAP)
    }

    fn bus_with_caps(
        event_log: Arc<dyn EventLogRepo>,
        observer_capacity: usize,
        ring_retention: usize,
    ) -> Arc<EventBus> {
        let config = Config {
            bus_observer_capacity: observer_capacity,
            bus_ring_retention: ring_retention,
            ..Config::default()
        };
        EventBus::with_config(event_log, &config)
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
    ) -> (Arc<EventBus>, Sandboxed<Arc<SqliteBackend>>) {
        let backend = sandboxed_backend([0xab; 32]).await.map(Arc::new);
        let event_log: Arc<dyn EventLogRepo> = Arc::new(BackedEventLog(Arc::clone(&backend)));
        let bus = bus_with_caps(event_log, channel_cap, ring_cap);
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
        let bus = null_bus();
        let mut sub = bus.subscribe();
        bus.record(core_event("silent.record"));
        assert!(matches!(sub.try_recv(), Ok(None)));
    }

    #[test]
    fn record_retains_event_in_ring_for_replay() {
        let bus = null_bus();
        let ev = core_event("recorded.candidate");
        let id = ev.id;
        bus.record(ev);
        assert_eq!(bus.lookup(id).map(|e| e.id), Some(id));
    }

    #[tokio::test]
    async fn record_then_replay_delivers_event_exactly_once() {
        let bus = null_bus();
        let mut rx = bus.subscribe();

        let ev = core_event("trigger.candidate");
        let id = ev.id;
        bus.record(ev);
        bus.replay_and_publish(id).await.unwrap();

        let first = rx.try_recv().unwrap().unwrap();
        assert!(
            first.replay,
            "the single delivery must be the replayed event"
        );
        assert_eq!(first.kind, "trigger.candidate");
        assert!(
            matches!(rx.try_recv(), Ok(None)),
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
        let bus = bus_with_caps(
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

    struct RecordingRepo {
        stored: Mutex<Vec<EventId>>,
    }

    impl RecordingRepo {
        fn new() -> Self {
            Self {
                stored: Mutex::new(Vec::new()),
            }
        }

        fn stored_ids(&self) -> Vec<EventId> {
            self.stored.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl EventLogRepo for RecordingRepo {
        async fn insert(&self, event: &Event) -> Result<(), StorageError> {
            tokio::task::yield_now().await;
            self.stored.lock().unwrap().push(event.id);
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

    fn recording_bus() -> (Arc<EventBus>, Arc<RecordingRepo>) {
        let repo = Arc::new(RecordingRepo::new());
        let bus = bus_with_caps(
            Arc::clone(&repo) as Arc<dyn EventLogRepo>,
            CHANNEL_CAP,
            RING_CAP,
        );
        (bus, repo)
    }

    #[tokio::test]
    async fn await_flush_resolves_only_after_shutdown_drain_persists_pending_events() {
        let (bus, repo) = recording_bus();
        EventBus::spawn_flush_task(Arc::clone(&bus));

        let ev1 = core_event("drain.a");
        let ev2 = core_event("drain.b");
        let ev3 = core_event("drain.c");
        let ids = [ev1.id, ev2.id, ev3.id];

        bus.publish(ev1);
        bus.publish(ev2);
        bus.publish(ev3);
        bus.shutdown();

        let completed = tokio::time::timeout(Duration::from_secs(5), bus.await_flush()).await;
        assert!(
            completed.is_ok(),
            "await_flush must resolve after the shutdown drain"
        );

        let stored = repo.stored_ids();
        for id in ids {
            assert!(
                stored.contains(&id),
                "event {id} must reach the repo before await_flush resolves"
            );
        }
    }

    #[tokio::test]
    async fn await_flush_returns_immediately_when_flush_task_never_spawned() {
        let bus = null_bus();
        let completed = tokio::time::timeout(Duration::from_secs(5), bus.await_flush()).await;
        assert!(
            completed.is_ok(),
            "await_flush must not hang when no flush task was ever spawned"
        );
    }

    #[tokio::test]
    async fn second_await_flush_returns_immediately_after_receiver_consumed() {
        let (bus, _repo) = recording_bus();
        EventBus::spawn_flush_task(Arc::clone(&bus));

        bus.publish(core_event("second.a"));
        bus.shutdown();

        let first = tokio::time::timeout(Duration::from_secs(5), bus.await_flush()).await;
        assert!(
            first.is_ok(),
            "first await_flush must resolve after the shutdown drain"
        );

        let second = tokio::time::timeout(Duration::from_secs(5), bus.await_flush()).await;
        assert!(
            second.is_ok(),
            "second await_flush must return immediately once the receiver is consumed"
        );
    }

    fn child_of(parent: &Event, kind: &str) -> Event {
        Event::caused_by(EventSource::Core, kind, serde_json::Value::Null, parent.id)
    }

    fn is_start(event: &Event) -> bool {
        event.kind == "action.start"
    }

    #[test]
    fn lineage_count_follows_only_the_path_of_the_given_event_through_a_branching_chain() {
        let bus = null_bus();
        let root = Event::new(EventSource::Twitch, "twitch.chat", serde_json::Value::Null);
        let first = child_of(&root, "action.start");
        let first_run = child_of(&first, "subaction.run");
        let left = child_of(&first_run, "action.start");
        let left_leaf = child_of(&left, "action.start");
        let right = child_of(&first_run, "action.start");
        let (left_leaf_id, right_id) = (left_leaf.id, right.id);
        for event in [root, first, first_run, left, left_leaf, right] {
            bus.publish(event);
        }

        let counts = (
            bus.count_in_lineage(left_leaf_id, is_start, 16),
            bus.count_in_lineage(right_id, is_start, 16),
        );

        assert_eq!(counts, (3, 2));
    }

    #[test]
    fn lineage_count_of_an_uncaused_event_ignores_unrelated_runs_in_the_ring() {
        let bus = null_bus();
        let mut parent = core_event("action.start");
        bus.publish(parent.clone());
        for _ in 0..20 {
            let next = child_of(&parent, "action.start");
            bus.publish(next.clone());
            parent = next;
        }
        let platform = Event::new(EventSource::Twitch, "twitch.chat", serde_json::Value::Null);
        let platform_id = platform.id;
        bus.publish(platform);

        assert_eq!(bus.count_in_lineage(platform_id, is_start, 16), 0);
    }

    #[test]
    fn lineage_count_stops_walking_at_the_ceiling() {
        let bus = null_bus();
        let mut parent = core_event("action.start");
        bus.publish(parent.clone());
        for _ in 0..9 {
            let next = child_of(&parent, "action.start");
            bus.publish(next.clone());
            parent = next;
        }

        assert_eq!(bus.count_in_lineage(parent.id, is_start, 4), 4);
    }

    #[test]
    fn published_event_is_in_the_ring_before_any_subscriber_can_receive_it() {
        let bus = null_bus();
        let mut rx = bus.subscribe();
        let ring_guard = bus.ring.lock().unwrap();
        let publisher = {
            let bus = Arc::clone(&bus);
            std::thread::spawn(move || bus.publish(core_event("ordering.probe")))
        };

        let deadline = std::time::Instant::now() + Duration::from_millis(50);
        let mut received_before_ring = false;
        while std::time::Instant::now() < deadline {
            if matches!(rx.try_recv(), Ok(Some(_))) {
                received_before_ring = true;
                break;
            }
            std::thread::yield_now();
        }
        drop(ring_guard);
        publisher.join().unwrap();

        assert!(
            !received_before_ring,
            "a subscriber received the event while the ring did not hold it yet"
        );
    }

    fn observer_loss(bus: &EventBus, consumer: &str) -> Option<u64> {
        bus.loss_report()
            .into_iter()
            .find(|entry| entry.consumer == consumer && entry.tier == DeliveryTier::Observer)
            .map(|entry| entry.loss.skipped)
    }

    #[tokio::test]
    async fn each_lagging_observer_counts_only_the_events_it_skipped_itself() {
        let bus = null_bus();
        let mut lagging = bus.subscribe_observer("lagging");
        let mut keeping_up = bus.subscribe_observer("keeping_up");
        let overflow = 6;
        for i in 0..CHANNEL_CAP / 2 {
            bus.publish(core_event(&format!("early.{i}")));
        }
        while let Ok(Some(_)) = keeping_up.try_recv() {}
        for i in 0..CHANNEL_CAP / 2 + overflow {
            bus.publish(core_event(&format!("late.{i}")));
        }

        let lagging_saw = lagging.next().await;
        let keeping_up_saw = keeping_up.next().await;

        assert!(matches!(lagging_saw, Delivery::Skipped(n) if n == overflow as u64));
        assert!(matches!(keeping_up_saw, Delivery::Event(_)));
        assert_eq!(
            (
                observer_loss(&bus, "lagging"),
                observer_loss(&bus, "keeping_up")
            ),
            (Some(overflow as u64), Some(0))
        );
    }

    #[tokio::test]
    async fn watch_loss_wakes_with_the_totals_of_a_critical_consumer_that_dropped() {
        let config = Config {
            critical_bulk_capacity: 1,
            ..Config::default()
        };
        let bus = EventBus::with_config(Arc::new(NullEventLogRepo), &config);
        let _critical = bus.subscribe_critical("probe");
        let mut watch = bus.watch_loss();
        for _ in 0..3 {
            bus.publish(core_event("action.start"));
        }

        let report = tokio::time::timeout(Duration::from_secs(5), watch.changed())
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        let dropped = report
            .iter()
            .find(|entry| entry.consumer == "probe")
            .map(|entry| entry.loss.bulk_dropped);
        assert_eq!(dropped, Some(2));
    }
}
