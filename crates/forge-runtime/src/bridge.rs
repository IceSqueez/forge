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
    use crate::{Config, NullEventLogRepo};
    use forge_events::{Event, EventSource};
    use forge_types::EventId;
    use futures_util::StreamExt as _;

    const OBSERVER_CAPACITY: usize = 4;
    const FLOOD: usize = OBSERVER_CAPACITY * 3;

    fn published(bus: &EventBus, count: usize) -> Vec<EventId> {
        (0..count)
            .map(|i| {
                let event = Event::new(
                    EventSource::Core,
                    format!("ev.{i}"),
                    serde_json::Value::Null,
                );
                let id = event.id;
                bus.publish(event);
                id
            })
            .collect()
    }

    #[tokio::test]
    async fn bus_subscription_delivers_events_in_publish_order() {
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let stream = bus_subscription(Arc::clone(&bus));
        tokio::pin!(stream);
        let ids = published(&bus, 2);

        let received = vec![
            stream.next().await.unwrap().id,
            stream.next().await.unwrap().id,
        ];

        assert_eq!(received, ids);
    }

    #[tokio::test]
    async fn a_lagged_bus_subscription_resumes_at_the_oldest_retained_event_instead_of_ending() {
        let config = Config {
            bus_observer_capacity: OBSERVER_CAPACITY,
            ..Config::default()
        };
        let bus = EventBus::with_config(Arc::new(NullEventLogRepo), &config);
        let stream = bus_subscription(Arc::clone(&bus));
        tokio::pin!(stream);
        let ids = published(&bus, FLOOD);

        let first = stream.next().await.map(|event| event.id);

        assert_eq!(first, Some(ids[FLOOD - OBSERVER_CAPACITY]));
    }
}
