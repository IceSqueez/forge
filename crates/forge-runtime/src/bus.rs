use crate::buf::RingBuffer;
use forge_events::{Event, EventPublisher, EventsError};
use forge_types::EventId;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::broadcast;

const CHANNEL_CAP: usize = 1_024;
const RING_CAP: usize = 10_000;

pub struct EventBus {
    sender: broadcast::Sender<Event>,
    ring: Mutex<RingBuffer<Event>>,
    total_published: AtomicU64,
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
    pub fn new() -> Arc<Self> {
        let (sender, _) = broadcast::channel(CHANNEL_CAP);
        Arc::new(Self {
            sender,
            ring: Mutex::new(RingBuffer::new(RING_CAP)),
            total_published: AtomicU64::new(0),
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
    pub fn replay(&self, event_id: EventId) -> Option<Event> {
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

    fn core_event(kind: &str) -> Event {
        Event::new(EventSource::Core, kind, serde_json::Value::Null)
    }

    #[tokio::test]
    async fn publish_subscribe_roundtrip() {
        let bus = EventBus::new();
        let mut sub = bus.subscribe();
        let ev = core_event("action.start");
        let expected_id = ev.id;
        bus.publish(ev);
        let received = sub.recv().await.unwrap();
        assert_eq!(received.id, expected_id);
    }

    #[tokio::test]
    async fn multiple_subscribers_each_receive_same_event() {
        let bus = EventBus::new();
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
        let bus = EventBus::new();
        for i in 0..RING_CAP + 5 {
            bus.publish(core_event(&format!("tick.{i}")));
        }
        let stats = bus.stats();
        assert_eq!(stats.ring_len, RING_CAP);
        assert_eq!(stats.total_published, (RING_CAP + 5) as u64);
    }

    #[test]
    fn replay_returns_stored_event_by_id() {
        let bus = EventBus::new();
        let ev = core_event("action.done");
        let id = ev.id;
        bus.publish(ev);
        let replayed = bus.replay(id);
        assert!(replayed.is_some());
        assert_eq!(replayed.unwrap().id, id);
    }

    #[test]
    fn replay_returns_none_for_missing_id() {
        let bus = EventBus::new();
        let ghost_id = EventId::new();
        assert!(bus.replay(ghost_id).is_none());
    }

    #[tokio::test]
    async fn lagged_subscriber_gets_lagging_error() {
        let bus = EventBus::new();
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
        let bus = EventBus::new();
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
        let bus = EventBus::new();
        bus.publish(core_event("x"));
        bus.publish(core_event("y"));
        let s = bus.stats();
        assert_eq!(s.total_published, 2);
        assert_eq!(s.ring_len, 2);
    }
}
