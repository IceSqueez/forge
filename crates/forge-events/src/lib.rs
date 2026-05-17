#![doc = "Event bus contract: Event, EventStream, EventSource taxonomy."]

pub mod bus;
pub mod source;

pub use bus::{EventStream, EventsError};
pub use source::EventSource;

use forge_types::EventId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub source: EventSource,
    /// Dotted hierarchical tag, e.g. `"chat.message"`, `"action.start"`.
    pub kind: String,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub payload: serde_json::Value,
    pub caused_by: Option<EventId>,
}

impl Event {
    pub fn new(source: EventSource, kind: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: EventId::new(),
            source,
            kind: kind.into(),
            timestamp: OffsetDateTime::now_utc(),
            payload,
            caused_by: None,
        }
    }

    pub fn caused_by(
        source: EventSource,
        kind: impl Into<String>,
        payload: serde_json::Value,
        parent: EventId,
    ) -> Self {
        Self {
            caused_by: Some(parent),
            ..Self::new(source, kind, payload)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn event_has_unique_id_per_construction() {
        let a = Event::new(EventSource::Core, "test.event", serde_json::Value::Null);
        let b = Event::new(EventSource::Core, "test.event", serde_json::Value::Null);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn caused_by_links_parent_event() {
        let parent = Event::new(EventSource::Twitch, "chat.message", serde_json::Value::Null);
        let parent_id = parent.id;
        let child = Event::caused_by(
            EventSource::Core,
            "command.matched",
            serde_json::Value::Null,
            parent_id,
        );
        assert_eq!(child.caused_by, Some(parent_id));
    }

    #[test]
    fn event_without_parent_has_no_caused_by() {
        let e = Event::new(EventSource::Core, "action.start", serde_json::Value::Null);
        assert!(e.caused_by.is_none());
    }

    #[test]
    fn event_serde_roundtrip() {
        let e = Event::new(
            EventSource::Obs,
            "scene.changed",
            serde_json::json!({"from": "Menu", "to": "Gameplay"}),
        );
        let json = serde_json::to_string(&e).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(e.id, back.id);
        assert_eq!(e.source, back.source);
        assert_eq!(e.kind, back.kind);
        assert_eq!(e.caused_by, back.caused_by);
    }

    #[test]
    fn event_causation_chain_field_present() {
        let e = Event::new(EventSource::Timer, "timer.tick", serde_json::Value::Null);
        let json = serde_json::to_value(&e).unwrap();
        assert!(
            json.get("caused_by").is_some(),
            "caused_by must always be present in serialized Event"
        );
    }
}
