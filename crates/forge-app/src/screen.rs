use forge_platform_core::IntegrationId;

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
