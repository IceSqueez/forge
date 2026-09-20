use std::fmt;

use forge_platform_core::BuiltinId;
use forge_types::{ActionId, TriggerInstanceId};

use crate::settings::SettingsSection;
use crate::tts::TtsSection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    Chat,
    Actions(Option<ActionId>),
    Triggers(Option<TriggerInstanceId>),
    Queues,
    EventFeed,
    Globals,
    Scripts,
    Platforms,
    StreamApps,
    BuiltinDetail(BuiltinId),
    Tts(Option<TtsSection>),
    Soundboard,
    Overlays,
    Server,
    Settings(Option<SettingsSection>),
}

impl Screen {
    pub fn same_nav(&self, other: &Screen) -> bool {
        matches!(
            (self, other),
            (Screen::Triggers(_), Screen::Triggers(_))
                | (Screen::Actions(_), Screen::Actions(_))
                | (Screen::Settings(_), Screen::Settings(_))
                | (Screen::Tts(_), Screen::Tts(_))
        ) || self == other
    }
}

const SCREEN_CLI_NAMES: &[&str] = &[
    "home",
    "chat",
    "actions",
    "triggers",
    "queues",
    "event-feed",
    "globals",
    "scripts",
    "platforms",
    "stream-apps",
    "builtin-detail",
    "tts",
    "soundboard",
    "overlays",
    "server",
    "settings",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenArgError {
    UnknownScreen(String),
    UnknownSection {
        screen: &'static str,
        section: String,
    },
    MissingBuiltinId,
    SelectNotSupported,
    InvalidSelectId,
}

impl fmt::Display for ScreenArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScreenArgError::UnknownScreen(name) => write!(
                f,
                "unknown screen '{name}'; accepted screens: {}",
                SCREEN_CLI_NAMES.join(", ")
            ),
            ScreenArgError::UnknownSection { screen, section } => {
                write!(f, "unknown {screen} section '{section}'")
            }
            ScreenArgError::MissingBuiltinId => {
                write!(
                    f,
                    "builtin-detail requires an id, e.g. builtin-detail:twitch"
                )
            }
            ScreenArgError::SelectNotSupported => write!(
                f,
                "--select is only supported for the actions and triggers screens"
            ),
            ScreenArgError::InvalidSelectId => write!(f, "--select value is not a valid id"),
        }
    }
}

impl Screen {
    pub fn accepted_cli_names() -> &'static [&'static str] {
        SCREEN_CLI_NAMES
    }

    pub fn parse_cli(arg: &str) -> Result<Screen, ScreenArgError> {
        let (name, param) = match arg.split_once(':') {
            Some((name, param)) => (name, Some(param)),
            None => (arg, None),
        };
        match name {
            "home" => Ok(Screen::Home),
            "chat" => Ok(Screen::Chat),
            "actions" => Ok(Screen::Actions(None)),
            "triggers" => Ok(Screen::Triggers(None)),
            "queues" => Ok(Screen::Queues),
            "event-feed" => Ok(Screen::EventFeed),
            "globals" => Ok(Screen::Globals),
            "scripts" => Ok(Screen::Scripts),
            "platforms" => Ok(Screen::Platforms),
            "stream-apps" => Ok(Screen::StreamApps),
            "builtin-detail" => param
                .map(|id| Screen::BuiltinDetail(BuiltinId::new(id)))
                .ok_or(ScreenArgError::MissingBuiltinId),
            "tts" => match param {
                None => Ok(Screen::Tts(None)),
                Some(key) => TtsSection::from_key(key)
                    .map(|section| Screen::Tts(Some(section)))
                    .ok_or(ScreenArgError::UnknownSection {
                        screen: "tts",
                        section: key.to_owned(),
                    }),
            },
            "soundboard" => Ok(Screen::Soundboard),
            "overlays" => Ok(Screen::Overlays),
            "server" => Ok(Screen::Server),
            "settings" => match param {
                None => Ok(Screen::Settings(None)),
                Some(key) => SettingsSection::from_key(key)
                    .map(|section| Screen::Settings(Some(section)))
                    .ok_or(ScreenArgError::UnknownSection {
                        screen: "settings",
                        section: key.to_owned(),
                    }),
            },
            _ => Err(ScreenArgError::UnknownScreen(name.to_owned())),
        }
    }

    pub fn with_selected_entity(self, raw: &str) -> Result<Screen, ScreenArgError> {
        match self {
            Screen::Actions(_) => raw
                .parse::<ActionId>()
                .map(|id| Screen::Actions(Some(id)))
                .map_err(|_| ScreenArgError::InvalidSelectId),
            Screen::Triggers(_) => raw
                .parse::<TriggerInstanceId>()
                .map(|id| Screen::Triggers(Some(id)))
                .map_err(|_| ScreenArgError::InvalidSelectId),
            _ => Err(ScreenArgError::SelectNotSupported),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_section_opened_directly_still_lights_the_settings_nav_leaf() {
        for (current, leaf, same, case) in [
            (
                Screen::Settings(Some(SettingsSection::WebSocket)),
                Screen::Settings(None),
                true,
                "a section reached without passing through the leaf",
            ),
            (
                Screen::Settings(Some(SettingsSection::WebSocket)),
                Screen::Server,
                false,
                "the screen the section was reached from",
            ),
        ] {
            assert_eq!(current.same_nav(&leaf), same, "{case}");
        }
    }
}
