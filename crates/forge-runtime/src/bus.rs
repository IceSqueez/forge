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
use tokio::sync::broadcast;

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

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

pub struct EventBus {
    sender: broadcast::Sender<Event>,
    ring: Mutex<RingBuffer<Event>>,
    total_published: AtomicU64,
    event_log: Arc<dyn EventLogRepo>,
}

pub struct BusStats {
    pub total_published: u64,
    pub ring_len: usize,
    pub subscriber_count: usize,
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

    pub fn stats(&self) -> BusStats {
        let ring = self.ring.lock().unwrap_or_else(|p| p.into_inner());
        BusStats {
            total_published: self.total_published.load(Ordering::Relaxed),
            ring_len: ring.len(),
            subscriber_count: self.sender.receiver_count(),
        }
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
        let stats = bus.stats();
        assert_eq!(stats.ring_len, RING_CAP);
        assert_eq!(stats.total_published, (RING_CAP + 5) as u64);
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

    #[test]
    fn stats_tracks_total_published_and_ring_len() {
        let bus = null_bus();
        bus.publish(core_event("x"));
        bus.publish(core_event("y"));
        let s = bus.stats();
        assert_eq!(s.total_published, 2);
        assert_eq!(s.ring_len, 2);
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
}
