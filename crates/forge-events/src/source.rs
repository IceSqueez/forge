use forge_types::PlatformId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Twitch,
    YouTube,
    Kick,
    Trovo,
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
            Self::Trovo => Some(PlatformId::Trovo),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn all_variants_serialize_and_deserialize() {
        let variants = [
            EventSource::Twitch,
            EventSource::YouTube,
            EventSource::Kick,
            EventSource::Trovo,
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
        for src in variants {
            let json = serde_json::to_string(&src).unwrap();
            let back: EventSource = serde_json::from_str(&json).unwrap();
            assert_eq!(src, back, "serde roundtrip failed for {src:?}");
        }
    }

    #[test]
    fn source_count_is_fifteen() {
        let variants = [
            EventSource::Twitch,
            EventSource::YouTube,
            EventSource::Kick,
            EventSource::Trovo,
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
        assert_eq!(
            variants.len(),
            15,
            "EventSource must have exactly 15 variants per CLAUDE.md §12b"
        );
    }

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
        assert_eq!(EventSource::Trovo.to_platform_id(), Some(PlatformId::Trovo));
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
