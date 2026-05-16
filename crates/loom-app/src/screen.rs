#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingStep {
    Welcome,
    ConnectPlatform,
    ConnectObs,
    StarterPack,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSection {
    Appearance,
    Language,
    Shortcuts,
    Notifications,
    Scripting,
    Queues,
    Storage,
    WebSocket,
    Version,
    Diagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Onboarding(OnboardingStep),
    Hub,
    LiveChat,
    EventFeed,
    Viewers,
    Globals,
    Actions,
    Commands,
    Platforms,
    StreamApps,
    Integrations,
    Tts,
    Soundboard,
    ScriptEditor,
    Server,
    Logs,
    Settings(SettingsSection),
}
