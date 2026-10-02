use std::fmt;

use forge_platform_core::IntegrationCategory;
use forge_types::IntegrationId;
use forge_types::{ActionId, TriggerInstanceId};

use crate::settings::SettingsSection;
use crate::tts::TtsSection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Home,
    Chat,
    Actions(Option<ActionId>),
    Triggers(Option<TriggerInstanceId>),
    Queues,
    EventFeed,
    Globals,
    Scripts,
    Integrations(Option<IntegrationCategory>),
    BuiltinDetail(IntegrationId),
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
                | (Screen::Integrations(_), Screen::Integrations(_))
        ) || self == other
    }
}

const SCREEN_CLI_NAMES: &[&str] = &[
    "welcome",
    "home",
    "chat",
    "actions",
    "triggers",
    "queues",
    "event-feed",
    "globals",
    "scripts",
    "integrations",
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
            "welcome" => Ok(Screen::Welcome),
            "home" => Ok(Screen::Home),
            "chat" => Ok(Screen::Chat),
            "actions" => Ok(Screen::Actions(None)),
            "triggers" => Ok(Screen::Triggers(None)),
            "queues" => Ok(Screen::Queues),
            "event-feed" => Ok(Screen::EventFeed),
            "globals" => Ok(Screen::Globals),
            "scripts" => Ok(Screen::Scripts),
            "integrations" => match param {
                None => Ok(Screen::Integrations(None)),
                Some(key) => IntegrationCategory::from_key(key)
                    .map(|category| Screen::Integrations(Some(category)))
                    .ok_or(ScreenArgError::UnknownSection {
                        screen: "integrations",
                        section: key.to_owned(),
                    }),
            },
            "builtin-detail" => param
                .map(|id| Screen::BuiltinDetail(IntegrationId::new(id)))
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
            (
                Screen::Integrations(Some(IntegrationCategory::AUDIO)),
                Screen::Integrations(None),
                true,
                "a category view of the hub",
            ),
        ] {
            assert_eq!(current.same_nav(&leaf), same, "{case}");
        }
    }

    #[test]
    fn the_integrations_screen_opens_on_the_hub_or_any_category() {
        assert_eq!(
            Screen::parse_cli("integrations"),
            Ok(Screen::Integrations(None))
        );
        for category in IntegrationCategory::DISPLAY_ORDER {
            assert_eq!(
                Screen::parse_cli(&format!("integrations:{}", category.key())),
                Ok(Screen::Integrations(Some(*category))),
                "{}",
                category.key()
            );
        }
    }

    #[test]
    fn an_unknown_integrations_category_is_rejected_naming_it() {
        for section in ["", "Streaming", "platforms", "stream_apps"] {
            assert_eq!(
                Screen::parse_cli(&format!("integrations:{section}")),
                Err(ScreenArgError::UnknownSection {
                    screen: "integrations",
                    section: section.to_owned(),
                })
            );
        }
    }

    #[test]
    fn every_advertised_screen_name_opens_a_screen() {
        for name in Screen::accepted_cli_names() {
            let parsed = Screen::parse_cli(name);
            assert!(
                parsed.is_ok() || parsed == Err(ScreenArgError::MissingBuiltinId),
                "{name} is advertised but rejected"
            );
        }
    }

    #[test]
    fn the_screen_names_the_hub_replaced_are_no_longer_accepted() {
        for retired in ["platforms", "stream-apps"] {
            assert_eq!(
                Screen::parse_cli(retired),
                Err(ScreenArgError::UnknownScreen(retired.to_owned()))
            );
            assert!(!Screen::accepted_cli_names().contains(&retired));
        }
    }

    #[test]
    fn the_welcome_name_opens_the_welcome_screen() {
        assert_eq!(Screen::parse_cli("welcome"), Ok(Screen::Welcome));
    }
}
