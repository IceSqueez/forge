use forge_components::Icon;

/// Top-level router discriminant: the shell renders exactly one screen at a time
/// behind fixed chrome. This is the migration seed roster —
/// the full parameterized roster (`ActionEditor(id)`, `BuiltinDetail(id)`,
/// sectioned `Tts`/`Settings`, error states) grows as each real screen lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Chat,
    Actions,
    Triggers,
    Platforms,
    Settings,
}

impl Screen {
    pub const SEED_ROSTER: [Screen; 6] = [
        Screen::Home,
        Screen::Chat,
        Screen::Actions,
        Screen::Triggers,
        Screen::Platforms,
        Screen::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Home => "Home",
            Screen::Chat => "Chat",
            Screen::Actions => "Actions",
            Screen::Triggers => "Triggers",
            Screen::Platforms => "Platforms",
            Screen::Settings => "Settings",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            Screen::Home => Icon::Home,
            Screen::Chat => Icon::MessageCircle,
            Screen::Actions => Icon::Bolt,
            Screen::Triggers => Icon::TargetArrow,
            Screen::Platforms => Icon::Broadcast,
            Screen::Settings => Icon::Settings,
        }
    }
}
