use forge_platform_core::BuiltinId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    Chat,
    Actions,
    Triggers,
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
