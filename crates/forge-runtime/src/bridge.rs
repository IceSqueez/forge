use crate::bus::{Delivery, EventBus};
use forge_events::Event;
use futures_core::Stream;
use futures_util::stream;
use std::sync::Arc;

/// Lagged items are skipped and counted in drop accounting; the lag count is logged at WARN.
pub fn bus_subscription(bus: Arc<EventBus>) -> impl Stream<Item = Event> + Send + 'static {
    stream::unfold(bus.subscribe(), |mut subscription| async move {
        loop {
            match subscription.next().await {
                Delivery::Event(event) => return Some((Arc::unwrap_or_clone(event), subscription)),
                Delivery::Skipped(missed) => {
                    tracing::warn!(missed, "event bus subscriber lagged");
                }
                Delivery::Closed => return None,
            }
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::NullEventLogRepo;
    use forge_events::{Event, EventSource};

    #[tokio::test]
    async fn bus_subscription_delivers_published_event() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let mut stream = bus_subscription(Arc::clone(&bus));
        let ev = Event::new(EventSource::Core, "action.start", serde_json::Value::Null);
        let expected_id = ev.id;
        bus.publish(ev);
        let received = stream.next().await.unwrap();
        assert_eq!(received.id, expected_id);
    }

    #[tokio::test]
    async fn bus_subscription_delivers_multiple_events_in_order() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
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
