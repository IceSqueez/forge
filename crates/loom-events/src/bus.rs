use crate::Event;
use async_trait::async_trait;
use loom_types::EventId;
use tokio::sync::broadcast;

#[derive(Debug, thiserror::Error)]
pub enum EventsError {
    #[error("event bus is closed")]
    BusClosed,

    #[error("event {0} not found in replay buffer")]
    ReplayMiss(EventId),

    #[error("subscriber is lagging; events were dropped")]
    LaggingReceiver,
}

pub struct EventStream(broadcast::Receiver<Event>);

impl EventStream {
    pub fn new(rx: broadcast::Receiver<Event>) -> Self {
        Self(rx)
    }

    pub async fn recv(&mut self) -> Result<Event, EventsError> {
        match self.0.recv().await {
            Ok(event) => Ok(event),
            Err(broadcast::error::RecvError::Closed) => Err(EventsError::BusClosed),
            Err(broadcast::error::RecvError::Lagged(_)) => Err(EventsError::LaggingReceiver),
        }
    }
}

#[async_trait]
pub trait EventBus: Send + Sync {
    /// Slow subscribers lag (broadcast semantics); publisher never blocks on them.
    async fn publish(&self, event: Event) -> Result<(), EventsError>;

    fn subscribe(&self) -> EventStream;

    /// Returns the captured event with original ID preserved.
    async fn replay(&self, id: EventId) -> Result<Event, EventsError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::EventSource;

    fn make_channel() -> (broadcast::Sender<Event>, EventStream) {
        let (tx, rx) = broadcast::channel(16);
        (tx, EventStream::new(rx))
    }

    #[tokio::test]
    async fn event_stream_receives_published_event() {
        let (tx, mut stream) = make_channel();
        let event = Event::new(EventSource::Core, "action.start", serde_json::Value::Null);
        let sent_id = event.id;
        tx.send(event).unwrap();
        let received = stream.recv().await.unwrap();
        assert_eq!(received.id, sent_id);
    }

    #[tokio::test]
    async fn event_stream_returns_bus_closed_when_sender_dropped() {
        let (tx, mut stream) = make_channel();
        drop(tx);
        let result = stream.recv().await;
        assert!(matches!(result, Err(EventsError::BusClosed)));
    }

    #[tokio::test]
    async fn event_stream_returns_lagging_on_overflow() {
        let (tx, mut stream) = make_channel();
        for i in 0..20u32 {
            let event = Event::new(EventSource::Core, "flood", serde_json::json!({"i": i}));
            let _ = tx.send(event);
        }
        let mut got_lagging = false;
        loop {
            match stream.recv().await {
                Ok(_) => {}
                Err(EventsError::LaggingReceiver) => {
                    got_lagging = true;
                    break;
                }
                Err(EventsError::BusClosed) | Err(_) => break,
            }
        }
        assert!(
            got_lagging,
            "lagging receiver must surface EventsError::LaggingReceiver"
        );
    }
}
