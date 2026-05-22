use forge_platform_core::IntegrationId;
use forge_types::ActionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Language,
    Shortcuts,
    Notifications,
    Audio,
    Platforms,
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
    Commands,
    Platforms,
    StreamApps,
    Integrations,
    IntegrationDetail(IntegrationId),
    Tts(TtsSection),
    Soundboard,
    ScriptEditor,
    Server,
    Logs,
    Settings(SettingsSection),
}
