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
