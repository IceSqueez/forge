use forge_platform_core::BuiltinId;
use forge_types::TriggerInstanceId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    Chat,
    Actions,
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
    Server,
    Settings,
}

impl Screen {
    pub fn same_nav(&self, other: &Screen) -> bool {
        matches!((self, other), (Screen::Triggers(_), Screen::Triggers(_))) || self == other
    }
}
