use forge_types::{EventId, IntegrationId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::{Event, EventSource};

pub const LATEST_CHANGED_KIND: &str = "latest.changed";
pub const NOW_PLAYING_KIND: &str = "music.now_playing";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestChanged {
    pub slot: String,
    pub platform: Option<String>,
    pub merged_changed: bool,
    pub cleared: bool,
}

impl LatestChanged {
    pub fn into_event(self, caused_by: Option<EventId>) -> Result<Event, serde_json::Error> {
        let payload = serde_json::to_value(self)?;
        Ok(match caused_by {
            Some(cause) => Event::caused_by(EventSource::Core, LATEST_CHANGED_KIND, payload, cause),
            None => Event::new(EventSource::Core, LATEST_CHANGED_KIND, payload),
        })
    }

    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != LATEST_CHANGED_KIND {
            return None;
        }
        serde_json::from_value(event.payload.clone()).ok()
    }
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

impl NowPlaying {
    pub fn into_event(self, source: EventSource) -> Result<Event, serde_json::Error> {
        Ok(Event::new(
            source,
            NOW_PLAYING_KIND,
            serde_json::to_value(self)?,
        ))
    }

    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != NOW_PLAYING_KIND {
            return None;
        }
        serde_json::from_value(event.payload.clone()).ok()
    }
}
