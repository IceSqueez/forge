use forge_types::IntegrationId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::TypedEvent;

pub const LATEST_CHANGED_KIND: &str = "latest.changed";
pub const NOW_PLAYING_KIND: &str = "music.now_playing";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestChanged {
    pub slot: String,
    pub platform: Option<String>,
    pub merged_changed: bool,
    pub cleared: bool,
}

impl TypedEvent for LatestChanged {
    const KIND: &'static str = LATEST_CHANGED_KIND;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackState {
    Playing,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NowPlaying {
    pub producer: IntegrationId,
    pub state: PlaybackState,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub album: Option<String>,
    #[serde(default)]
    pub art: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
}

impl TypedEvent for NowPlaying {
    const KIND: &'static str = NOW_PLAYING_KIND;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn change() -> LatestChanged {
        LatestChanged {
            slot: "donation".to_owned(),
            platform: Some("twitch".to_owned()),
            merged_changed: true,
            cleared: false,
        }
    }

    #[test]
    fn latest_changed_event_carries_its_cause_and_decodes_back() {
        let cause = EventId::new();

        let event = change().into_event(Some(cause)).unwrap();

        assert_eq!(
            (
                event.source,
                event.caused_by,
                LatestChanged::from_event(&event)
            ),
            (EventSource::Core, Some(cause), Some(change()))
        );
    }

    #[test]
    fn decoders_ignore_events_of_another_kind() {
        let foreign = Event::new(
            EventSource::Core,
            "global.set",
            serde_json::to_value(change()).unwrap(),
        );

        assert_eq!(LatestChanged::from_event(&foreign), None);
        assert_eq!(NowPlaying::from_event(&foreign), None);
    }

    #[test]
    fn producer_now_playing_payload_decodes_with_optional_fields_absent() {
        let event = Event::new(
            EventSource::Server,
            NOW_PLAYING_KIND,
            json!({
                "producer": "companion",
                "state": "stopped",
                "occurred_at": "2026-10-03T12:00:00Z",
            }),
        );

        let decoded = NowPlaying::from_event(&event).unwrap();

        assert_eq!(
            (
                decoded.producer.as_str(),
                decoded.state,
                decoded.title.as_str(),
                decoded.album
            ),
            ("companion", PlaybackState::Stopped, "", None)
        );
    }
}
