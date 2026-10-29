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
mod timer;
mod typed;

pub use bus::EventsError;
pub use channel::{Delivery, EventChannel, EventObserver, EventSubscription, LagRecorder};
pub use chat_send::{
    ActingSender, ChatSendFailed, ChatSendRequest, ChatSent, SENT_AS_FIELD, SenderRole,
};
pub use connection::{ConnectionState, PlatformConnectionChanged};
pub use donation::{DONATION_RECEIVED_KIND, DonationReceived};
pub use globals::{
    GlobalArrayAppended, GlobalArrayRemoved, GlobalDecremented, GlobalDeleted, GlobalIncremented,
    GlobalSet, GlobalToggled,
};
pub use lane::DeliveryLane;
pub use latest::{LATEST_CHANGED_KIND, LatestChanged, NOW_PLAYING_KIND, NowPlaying, PlaybackState};
pub use publisher::EventPublisher;
pub use request::RequestFailed;
pub use run::{
    ActionDone, ActionSkipped, ActionStarted, QuickActionDone, RunOutcome, SubActionDone,
    SubActionRun,
};
pub use script::{
    ScriptFailed, ScriptHttpCall, ScriptHttpMethod, ScriptLogLevel, ScriptLogged, ScriptReloaded,
    ScriptStarted,
};
pub use source::EventSource;
pub use timer::TimerTick;
pub use typed::TypedEvent;

use forge_types::EventId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}
