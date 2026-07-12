use forge_components::Icon;

/// Top-level router discriminant: the shell renders exactly one screen at a time
/// behind fixed chrome. Every sidebar destination is a variant
/// here; navigation swaps the active-screen child entity. Parameterized detail
/// screens (`ActionEditor(id)`, sectioned `Tts`/`Settings`, error states) widen
/// this enum as each real screen lands — today each variant routes to a stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    Twitch,
    YouTube,
    Kick,
    Obs,
    VTube,
    Tts,
    Soundboard,
    Midi,
    Hotkeys,
    Discord,
    Server,
    Settings,
}

impl Screen {
    pub fn title(self) -> &'static str {
        match self {
            Screen::Home => "Home",
            Screen::Chat => "Chat",
            Screen::Actions => "Actions",
            Screen::Triggers => "Triggers",
            Screen::Queues => "Queues",
            Screen::EventFeed => "Event feed",
            Screen::Globals => "Globals",
            Screen::Scripts => "Scripts",
            Screen::Platforms => "Platforms",
            Screen::Twitch => "Twitch",
            Screen::YouTube => "YouTube",
            Screen::Kick => "Kick",
            Screen::Obs => "OBS Studio",
            Screen::VTube => "VTube Studio",
            Screen::Tts => "Text-to-Speech",
            Screen::Soundboard => "Soundboard",
            Screen::Midi => "MIDI",
            Screen::Hotkeys => "Hotkeys",
            Screen::Discord => "Discord",
            Screen::Server => "WebSocket server",
            Screen::Settings => "Settings",
        }
    }

    /// Glyph shown in the routed screen's placeholder header. Platform and
    /// stream-app screens share the broadcast glyph (their sidebar entry carries a
    /// brand dot instead of an icon); every other screen maps to its nav glyph.
    pub fn icon(self) -> Icon {
        match self {
            Screen::Home => Icon::Home,
            Screen::Chat => Icon::MessageCircle,
            Screen::Actions => Icon::Bolt,
            Screen::Triggers => Icon::TargetArrow,
            Screen::Queues => Icon::Notebook,
            Screen::EventFeed => Icon::Activity,
            Screen::Globals => Icon::Variable,
            Screen::Scripts => Icon::FileCode,
            Screen::Platforms
            | Screen::Twitch
            | Screen::YouTube
            | Screen::Kick
            | Screen::Obs
            | Screen::VTube => Icon::Broadcast,
            Screen::Tts => Icon::Volume,
            Screen::Soundboard => Icon::Music,
            Screen::Midi => Icon::PlugConnected,
            Screen::Hotkeys => Icon::Keyboard,
            Screen::Discord => Icon::Send,
            Screen::Server => Icon::Server,
            Screen::Settings => Icon::Settings,
        }
    }
}
