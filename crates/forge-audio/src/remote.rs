use std::fmt;

use async_trait::async_trait;
use forge_types::Redacted;

use crate::error::AudioError;

const WAVE_MEDIA_TYPE: &str = "audio/wav";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteDestinationId(String);

impl RemoteDestinationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RemoteDestinationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RemoteClipId(String);

impl RemoteClipId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RemoteClipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RemoteClipId({Redacted:?})")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipMediaType {
    Wave,
}

impl ClipMediaType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wave => WAVE_MEDIA_TYPE,
        }
    }
}

impl fmt::Display for ClipMediaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct RemoteClip {
    pub bytes: Vec<u8>,
    pub media_type: ClipMediaType,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteCommand {
    Stop,
    Pause,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDelivery {
    pub clip_id: RemoteClipId,
    pub live_players: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteVerdict {
    Played,
    Refused { reason: String },
    Unknown { reason: String },
}

#[async_trait]
pub trait RemoteAudioDestination: Send + Sync {
    async fn deliver(
        &self,
        destination: &RemoteDestinationId,
        clip: RemoteClip,
    ) -> Result<RemoteDelivery, AudioError>;

    async fn control(
        &self,
        destination: &RemoteDestinationId,
        clip_id: &RemoteClipId,
        command: RemoteCommand,
    ) -> Result<(), AudioError>;

    async fn verdict(&self, clip_id: &RemoteClipId) -> Result<RemoteVerdict, AudioError>;
}

#[cfg(test)]
mod tests {
    use forge_types::STAMP;

    use super::*;

    const CAPABILITY: &str = "clip-cap-7Hq2ZtVn9Lx4";

    #[test]
    fn clip_id_withholds_the_capability_from_every_debug_rendering() {
        let clip_id = RemoteClipId::new(CAPABILITY);
        assert_eq!(clip_id.expose(), CAPABILITY);

        let delivery = RemoteDelivery {
            clip_id: clip_id.clone(),
            live_players: 1,
        };
        for rendering in [format!("{clip_id:?}"), format!("{delivery:?}")] {
            assert!(
                !rendering.contains(CAPABILITY),
                "{rendering} leaks the capability"
            );
            assert!(
                rendering.contains(STAMP),
                "{rendering} does not mark the value as withheld"
            );
        }
    }

    #[test]
    fn wave_clips_advertise_the_media_type_the_browser_decodes() {
        assert_eq!(ClipMediaType::Wave.as_str(), "audio/wav");
    }
}
