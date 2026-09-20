use forge_platform_core::BuiltinId;
use forge_types::{ActionId, TriggerInstanceId};

use crate::settings::SettingsSection;

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
    Tts,
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
        ) || self == other
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
