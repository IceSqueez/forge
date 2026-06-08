use forge_types::PlatformId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Twitch,
    YouTube,
    Kick,
    Core,
    Rhai,
    Http,
    Obs,
    VTube,
    Discord,
    Midi,
    Hotkey,
    Timer,
    Server,
    Audio,
}

impl EventSource {
    pub fn to_platform_id(self) -> Option<PlatformId> {
        match self {
            Self::Twitch => Some(PlatformId::Twitch),
            Self::YouTube => Some(PlatformId::YouTube),
            Self::Kick => Some(PlatformId::Kick),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn to_platform_id_for_chat_sources_returns_some() {
        assert_eq!(
            EventSource::Twitch.to_platform_id(),
            Some(PlatformId::Twitch)
        );
        assert_eq!(
            EventSource::YouTube.to_platform_id(),
            Some(PlatformId::YouTube)
        );
        assert_eq!(EventSource::Kick.to_platform_id(), Some(PlatformId::Kick));
    }

    #[test]
    fn to_platform_id_for_core_sources_returns_none() {
        let non_platform = [
            EventSource::Core,
            EventSource::Rhai,
            EventSource::Http,
            EventSource::Obs,
            EventSource::VTube,
            EventSource::Discord,
            EventSource::Midi,
            EventSource::Hotkey,
            EventSource::Timer,
            EventSource::Server,
            EventSource::Audio,
        ];
        for src in non_platform {
            assert_eq!(
                src.to_platform_id(),
                None,
                "{src:?} should not map to a PlatformId"
            );
        }
    }
}
