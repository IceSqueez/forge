use async_trait::async_trait;
use loom_events::{Event, EventBus, EventStream, EventsError};
use loom_types::EventId;
use tokio::sync::broadcast;

pub struct InMemoryEventBus {
    tx: broadcast::Sender<Event>,
}

impl InMemoryEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    /// Slow subscribers lag (broadcast semantics); publisher never blocks on them.
    async fn publish(&self, event: Event) -> Result<(), EventsError> {
        self.tx
            .send(event)
            .map(|_| ())
            .map_err(|_| EventsError::BusClosed)
    }

    fn subscribe(&self) -> EventStream {
        EventStream::new(self.tx.subscribe())
    }

    async fn replay(&self, id: EventId) -> Result<Event, EventsError> {
        Err(EventsError::ReplayMiss(id))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use loom_events::EventSource;

    #[tokio::test]
    async fn pub_sub_round_trip() {
        let bus = InMemoryEventBus::new();
        let mut stream = bus.subscribe();
        let event = Event::new(EventSource::Core, "action.start", serde_json::Value::Null);
        let expected_id = event.id;
        bus.publish(event).await.unwrap();
        let received = stream.recv().await.unwrap();
        assert_eq!(received.id, expected_id);
    }

    #[tokio::test]
    async fn multiple_subscribers_each_receive() {
        let bus = InMemoryEventBus::new();
        let mut stream_a = bus.subscribe();
        let mut stream_b = bus.subscribe();
        let event = Event::new(EventSource::Core, "queue.paused", serde_json::Value::Null);
        let expected_id = event.id;
        bus.publish(event).await.unwrap();
        let a = stream_a.recv().await.unwrap();
        let b = stream_b.recv().await.unwrap();
        assert_eq!(a.id, expected_id);
        assert_eq!(b.id, expected_id);
    }
}
