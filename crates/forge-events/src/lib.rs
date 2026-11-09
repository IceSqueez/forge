mod bus;
mod channel;
mod chat_send;
mod connection;
mod donation;
mod globals;
mod lane;
mod latest;
mod publisher;
mod request;
mod run;
mod script;
mod source;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
mod timer;
mod trigger;
mod typed;

pub use bus::EventsError;
pub use channel::{
    Delivery, EventChannel, EventObserver, EventSubscription, LagRecorder,
    PLATFORM_EVENT_CHANNEL_CAPACITY,
};
pub use chat_send::{
    ActingSender, ChatSendFailed, ChatSendRequest, ChatSent, SENT_AS_FIELD, SenderRole, WhisperSent,
};
pub use connection::{ConnectionState, PlatformConnectionChanged};
pub use donation::{DONATION_PROVIDER_CONFIG_KEY, DONATION_RECEIVED_KIND, DonationReceived};
pub use globals::{
    GlobalArrayAppended, GlobalArrayRemoved, GlobalDecremented, GlobalDeleted, GlobalIncremented,
    GlobalSet, GlobalToggled,
};
pub use lane::DeliveryLane;
pub use latest::{LatestChanged, NOW_PLAYING_KIND, NowPlaying, PlaybackState};
pub use publisher::EventPublisher;
pub use request::RequestFailed;
pub use run::{
    ActionDone, ActionSkipped, ActionStarted, ManualTestRun, QuickActionDone, RunOutcome,
    SubActionDone, SubActionRun,
};
pub use script::{
    CUSTOM_EVENT_PREFIX, ScriptFailed, ScriptHttpCall, ScriptHttpMethod, ScriptLogLevel,
    ScriptLogged, ScriptReloaded, ScriptStarted,
};
pub use source::{EventSource, UnknownEventSource};
pub use timer::TimerTick;
pub use trigger::CommandMatched;
pub use typed::TypedEvent;

use std::fmt;

use forge_types::EventId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

const TEST_MARKER: &str = "test";

#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub source: EventSource,
    pub kind: String,
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    pub payload: serde_json::Value,
    pub caused_by: Option<EventId>,
    #[serde(default)]
    pub replay: bool,
    #[serde(default)]
    pub causation_depth: u16,
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Event")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("kind", &self.kind)
            .field("timestamp", &self.timestamp)
            .field("caused_by", &self.caused_by)
            .field("replay", &self.replay)
            .field("causation_depth", &self.causation_depth)
            .finish_non_exhaustive()
    }
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
            replay: false,
            causation_depth: 0,
        }
    }

    pub fn child_of(
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

    pub fn is_rehearsal(&self) -> bool {
        self.replay
            || self
                .payload
                .get(TEST_MARKER)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
    }

    pub fn mark_test_in_object_payload(&mut self) {
        if let Some(fields) = self.payload.as_object_mut() {
            fields.insert(TEST_MARKER.to_owned(), serde_json::Value::Bool(true));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_of_an_event_never_prints_its_payload() {
        let event = Event::new(
            EventSource::Twitch,
            "chat.message",
            serde_json::json!({ "message": "viewer-private-text" }),
        );

        let printed = format!("{event:?}");

        assert!(printed.contains("chat.message"), "{printed}");
        assert!(!printed.contains("viewer-private-text"), "{printed}");
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

    #[test]
    fn event_replay_backward_compat_missing_field() {
        let json = serde_json::json!({
            "id": "01900000000000000000000000",
            "source": "core",
            "kind": "action.start",
            "timestamp": "1970-01-01T00:00:00Z",
            "payload": null,
            "caused_by": null
        });
        let e: Event = serde_json::from_value(json).unwrap();
        assert!(
            !e.replay,
            "events without replay field must deserialize as false"
        );
    }

    #[test]
    fn a_rehearsal_is_a_replay_or_an_event_whose_payload_says_test_true() {
        use serde_json::json;
        for (replay, payload, expected) in [
            (false, json!({ "message": "hi" }), false),
            (true, json!({ "message": "hi" }), true),
            (false, json!({ "test": true }), true),
            (false, json!({ "test": false }), false),
            (false, json!({ "test": "true" }), false),
            (false, json!({ "test": 1 }), false),
            (false, json!({ "nested": { "test": true } }), false),
            (false, serde_json::Value::Null, false),
            (true, serde_json::Value::Null, true),
        ] {
            let mut event = Event::new(EventSource::Twitch, "chat.message", payload.clone());
            event.replay = replay;

            assert_eq!(
                event.is_rehearsal(),
                expected,
                "replay {replay}, payload {payload}"
            );
        }
    }

    #[test]
    fn marking_an_object_payload_as_test_makes_it_a_rehearsal_and_keeps_its_fields() {
        let mut event = Event::new(
            EventSource::Core,
            "action.start",
            serde_json::json!({ "action_name": "greet", "test": false }),
        );

        event.mark_test_in_object_payload();

        assert_eq!(
            (event.is_rehearsal(), event.payload["action_name"].as_str()),
            (true, Some("greet"))
        );
    }

    #[test]
    fn marking_a_non_object_payload_leaves_it_untouched() {
        for payload in [
            serde_json::Value::Null,
            serde_json::json!("text"),
            serde_json::json!([true]),
        ] {
            let mut event = Event::new(EventSource::Core, "action.start", payload.clone());

            event.mark_test_in_object_payload();

            assert_eq!(event.payload, payload);
        }
    }
}
