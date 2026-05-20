use forge_platform_core::IntegrationId;
use forge_types::ActionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Language,
    Shortcuts,
    Notifications,
    Platforms,
    Scripting,
    Queues,
    Storage,
    WebSocket,
    Version,
    Diagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    LiveChat,
    EventFeed,
    Viewers,
    Globals,
    Actions,
    ActionEditor(ActionId),
    Queues,
    Commands,
    Platforms,
    StreamApps,
    Integrations,
    IntegrationDetail(IntegrationId),
    Tts,
    Soundboard,
    ScriptEditor,
    Server,
    Logs,
    Settings(SettingsSection),
}
