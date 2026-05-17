use crate::bus::EventBus;
use forge_events::Event;
use futures_core::Stream;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{StreamExt as _, wrappers::errors::BroadcastStreamRecvError};

/// Wraps the bus into a `Stream<Item = Event>`. Lagged items are silently dropped;
/// lag count is logged at WARN so operators can tune `CHANNEL_CAP`.
pub fn bus_subscription(bus: Arc<EventBus>) -> impl Stream<Item = Event> + Send + 'static {
    let receiver = bus.subscribe().into_receiver();
    BroadcastStream::new(receiver).filter_map(|result| match result {
        Ok(event) => Some(event),
        Err(BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!(missed = n, "event bus subscriber lagged");
            None
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};

    #[tokio::test]
    async fn bus_subscription_delivers_published_event() {
        let bus = EventBus::new();
        let mut stream = bus_subscription(Arc::clone(&bus));
        let ev = Event::new(EventSource::Core, "action.start", serde_json::Value::Null);
        let expected_id = ev.id;
        bus.publish(ev);
        let received = stream.next().await.unwrap();
        assert_eq!(received.id, expected_id);
    }

    #[tokio::test]
    async fn bus_subscription_delivers_multiple_events_in_order() {
        let bus = EventBus::new();
        let mut stream = bus_subscription(Arc::clone(&bus));
        let ev1 = Event::new(EventSource::Core, "ev.1", serde_json::Value::Null);
        let ev2 = Event::new(EventSource::Core, "ev.2", serde_json::Value::Null);
        let id1 = ev1.id;
        let id2 = ev2.id;
        bus.publish(ev1);
        bus.publish(ev2);
        assert_eq!(stream.next().await.unwrap().id, id1);
        assert_eq!(stream.next().await.unwrap().id, id2);
    }
}
