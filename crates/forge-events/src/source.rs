use std::str::FromStr;

use forge_types::PlatformId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown event source {0:?}")]
pub struct UnknownEventSource(pub String);

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
    pub const ALL: [Self; 15] = [
        Self::Twitch,
        Self::YouTube,
        Self::Kick,
        Self::Core,
        Self::Rhai,
        Self::Http,
        Self::Obs,
        Self::VTube,
        Self::Discord,
        Self::Midi,
        Self::Hotkey,
        Self::Timer,
        Self::Server,
        Self::Audio,
        Self::Donation,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Twitch => "twitch",
            Self::YouTube => "youtube",
            Self::Kick => "kick",
            Self::Core => "core",
            Self::Rhai => "rhai",
            Self::Http => "http",
            Self::Obs => "obs",
            Self::VTube => "vtube",
            Self::Discord => "discord",
            Self::Midi => "midi",
            Self::Hotkey => "hotkey",
            Self::Timer => "timer",
            Self::Server => "server",
            Self::Audio => "audio",
            Self::Donation => "donation",
        }
    }

    pub fn to_platform_id(self) -> Option<PlatformId> {
        match self {
            Self::Twitch => Some(PlatformId::Twitch),
            Self::YouTube => Some(PlatformId::YouTube),
            Self::Kick => Some(PlatformId::Kick),
            Self::Core
            | Self::Rhai
            | Self::Http
            | Self::Obs
            | Self::VTube
            | Self::Discord
            | Self::Midi
            | Self::Hotkey
            | Self::Timer
            | Self::Server
            | Self::Audio
            | Self::Donation => None,
        }
    }
}

impl FromStr for EventSource {
    type Err = UnknownEventSource;

    fn from_str(wire_name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == wire_name)
            .ok_or_else(|| UnknownEventSource(wire_name.to_owned()))
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

    #[test]
    fn every_source_has_one_spelling_across_as_str_serde_and_from_str() {
        for source in EventSource::ALL {
            assert_eq!(
                serde_json::to_value(source).unwrap(),
                serde_json::json!(source.as_str()),
                "{source:?}"
            );
            assert_eq!(source.as_str().parse::<EventSource>().ok(), Some(source));
        }
    }

    #[test]
    fn a_spelling_outside_the_roster_is_refused() {
        assert!("Twitch".parse::<EventSource>().is_err());
        assert!("".parse::<EventSource>().is_err());
    }
}
