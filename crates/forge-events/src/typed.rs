use forge_types::EventId;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{Event, EventPublisher, EventSource};

pub trait TypedEvent: Serialize + DeserializeOwned {
    const KIND: &'static str;

    fn into_event(self, source: EventSource, caused_by: Option<EventId>) -> Event {
        let payload = serde_json::to_value(&self).unwrap_or_else(|error| {
            tracing::error!(
                kind = Self::KIND,
                category = ?error.classify(),
                "event payload could not be encoded"
            );
            serde_json::Value::Null
        });
        match caused_by {
            Some(parent) => Event::caused_by(source, Self::KIND, payload, parent),
            None => Event::new(source, Self::KIND, payload),
        }
    }

    fn publish(
        self,
        publisher: &dyn EventPublisher,
        source: EventSource,
        caused_by: Option<EventId>,
    ) {
        publisher.publish(self.into_event(source, caused_by));
    }

    fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Self::KIND {
            return None;
        }
        match serde_json::from_value(event.payload.clone()) {
            Ok(payload) => Some(payload),
            Err(error) => {
                tracing::warn!(
                    kind = Self::KIND,
                    event = %event.id,
                    category = ?error.classify(),
                    "event payload does not match its kind"
                );
                None
            }
        }
    }
}
