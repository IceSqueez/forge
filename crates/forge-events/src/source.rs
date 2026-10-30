use forge_types::PlatformId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Twitch,
    #[serde(rename = "youtube")]
    YouTube,
    Kick,
    Core,
    Rhai,
    Http,
    Obs,
    #[serde(rename = "vtube")]
    VTube,
    Discord,
    Midi,
    Hotkey,
    Timer,
    Server,
    Audio,
    Donation,
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

impl From<PlatformId> for EventSource {
    fn from(platform: PlatformId) -> Self {
        match platform {
            PlatformId::Twitch => Self::Twitch,
            PlatformId::YouTube => Self::YouTube,
            PlatformId::Kick => Self::Kick,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_id_converts_to_the_event_source_with_its_wire_spelling_and_back() {
        for platform in PlatformId::ALL {
            let source = EventSource::from(platform);
            assert_eq!(
                (
                    serde_json::to_value(source).unwrap(),
                    source.to_platform_id()
                ),
                (serde_json::json!(platform.as_str()), Some(platform)),
                "{platform:?}"
            );
        }
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
            EventSource::Donation,
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
