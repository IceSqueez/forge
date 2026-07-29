use crate::config;
use crate::descriptor::OverlayConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewShape {
    BadgeBanner,
    BorderedFrame,
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewAccent {
    Mauve,
    Sky,
    Green,
    Peach,
    Yellow,
    Red,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewFont {
    Sans,
    Mono,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewPosition {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewLineRole {
    Headline,
    Subline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLine {
    pub role: PreviewLineRole,
    pub text: String,
}

/// Text carries whatever the config holds; `%var%` tokens are expanded by the caller, not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewComposition {
    pub shape: PreviewShape,
    pub accent: PreviewAccent,
    pub font: PreviewFont,
    pub position: PreviewPosition,
    pub lines: Vec<PreviewLine>,
}

pub(crate) fn compose(shape: PreviewShape, config: &OverlayConfig) -> PreviewComposition {
    PreviewComposition {
        shape,
        accent: accent_of(config),
        font: font_of(config),
        position: position_of(config),
        lines: lines_of(config),
    }
}

fn accent_of(config: &OverlayConfig) -> PreviewAccent {
    match config::read_str(config, config::ACCENT) {
        "sky" => PreviewAccent::Sky,
        "green" => PreviewAccent::Green,
        "peach" => PreviewAccent::Peach,
        "yellow" => PreviewAccent::Yellow,
        "red" => PreviewAccent::Red,
        _ => PreviewAccent::Mauve,
    }
}

fn font_of(config: &OverlayConfig) -> PreviewFont {
    match config::read_str(config, config::FONT) {
        "JetBrains Mono" => PreviewFont::Mono,
        _ => PreviewFont::Sans,
    }
}

fn position_of(config: &OverlayConfig) -> PreviewPosition {
    match config::read_str(config, config::POSITION) {
        "top" => PreviewPosition::Top,
        "bottom" => PreviewPosition::Bottom,
        _ => PreviewPosition::Center,
    }
}

fn lines_of(config: &OverlayConfig) -> Vec<PreviewLine> {
    [
        (PreviewLineRole::Headline, config::HEADLINE),
        (PreviewLineRole::Subline, config::SUBLINE),
    ]
    .into_iter()
    .filter_map(|(role, key)| {
        let text = config::read_str(config, key);
        (!text.is_empty()).then(|| PreviewLine {
            role,
            text: text.to_owned(),
        })
    })
    .collect()
}
