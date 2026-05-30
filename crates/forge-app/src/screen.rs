use forge_platform_core::BuiltinId;
use forge_types::{ActionId, PlatformId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Language,
    Shortcuts,
    Notifications,
    Audio,
    Scripting,
    Queues,
    Storage,
    WebSocket,
    Version,
    Diagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TtsSection {
    Dashboard,
    Engines,
    Aliases,
    Filters,
    Triggers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    LiveChat,
    EventFeed,
    Globals,
    Actions,
    ActionEditor(ActionId),
    Queues,
    TriggersRegistry,
    Platforms,
    DeviceCodeFlow(PlatformId),
    StreamApps,
    Builtin,
    BuiltinDetail(BuiltinId),
    Tts(TtsSection),
    Soundboard,
    ScriptEditor,
    Server,
    Logs,
    Settings(SettingsSection),
}
