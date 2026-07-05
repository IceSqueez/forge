use iced::{Color, Element};

use crate::icons::{Icon, tabler_icon};
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::FONT_XS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticState {
    Enabled,
    Disabled,
    Connected,
    Disconnected,
    Running,
    Paused,
    Error,
}

impl SemanticState {
    pub fn color(self, palette: &ForgePalette) -> Color {
        match self {
            SemanticState::Enabled | SemanticState::Connected => palette.success,
            SemanticState::Disabled | SemanticState::Disconnected => palette.text_faint,
            SemanticState::Running => palette.brand,
            SemanticState::Paused => palette.warning,
            SemanticState::Error => palette.random,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SemanticState::Enabled => "Enabled",
            SemanticState::Disabled => "Disabled",
            SemanticState::Connected => "Connected",
            SemanticState::Disconnected => "Disconnected",
            SemanticState::Running => "Running",
            SemanticState::Paused => "Paused",
            SemanticState::Error => "Error",
        }
    }
}

pub fn state_icon<'a, Msg: 'a>(state: SemanticState, palette: &ForgePalette) -> Element<'a, Msg> {
    let color = state.color(palette);
    match state {
        SemanticState::Enabled => tabler_icon(Icon::CircleCheckFilled, FONT_XS, color),
        SemanticState::Disabled => tabler_icon(Icon::Circle, FONT_XS, color),
        SemanticState::Connected | SemanticState::Disconnected => status_dot(color, 7.0),
        SemanticState::Running => tabler_icon(Icon::Loader2, FONT_XS, color),
        SemanticState::Paused => tabler_icon(Icon::PlayerPause, FONT_XS, color),
        SemanticState::Error => tabler_icon(Icon::CircleX, FONT_XS, color),
    }
}
