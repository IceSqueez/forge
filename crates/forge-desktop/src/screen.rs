use forge_components::Icon;
use forge_platform_core::BuiltinId;

/// Top-level router discriminant: the shell renders exactly one screen at a time
/// behind fixed chrome. Every sidebar destination is a variant here; navigation
/// swaps the active-screen child entity. Platforms and Stream Apps each get their
/// own overview variant, but the per-integration detail is a single parameterized
/// [`Screen::BuiltinDetail`], which the one generic integration-detail view renders
/// from the target's four `Builtin*` traits. The enum is `Clone` (not `Copy`)
/// because `BuiltinDetail` carries an owned [`BuiltinId`].
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
    Midi,
    Hotkeys,
    Discord,
    Server,
    Settings,
}

impl Screen {
    pub fn title(&self) -> &'static str {
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
            Screen::StreamApps => "Stream apps",
            Screen::BuiltinDetail(_) => "Integration",
            Screen::Tts => "Text-to-Speech",
            Screen::Soundboard => "Soundboard",
            Screen::Midi => "MIDI",
            Screen::Hotkeys => "Hotkeys",
            Screen::Discord => "Discord",
            Screen::Server => "WebSocket server",
            Screen::Settings => "Settings",
        }
    }

    /// Glyph shown in the routed screen's placeholder header. The Platforms overview
    /// and every integration detail share the broadcast glyph (their sidebar entries
    /// carry a brand dot instead of an icon); every other screen maps to its nav
    /// glyph.
    pub fn icon(&self) -> Icon {
        match self {
            Screen::Home => Icon::Home,
            Screen::Chat => Icon::MessageCircle,
            Screen::Actions => Icon::Bolt,
            Screen::Triggers => Icon::TargetArrow,
            Screen::Queues => Icon::Notebook,
            Screen::EventFeed => Icon::Activity,
            Screen::Globals => Icon::Variable,
            Screen::Scripts => Icon::FileCode,
            Screen::Platforms | Screen::StreamApps | Screen::BuiltinDetail(_) => Icon::Broadcast,
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
