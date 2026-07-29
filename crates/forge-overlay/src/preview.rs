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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_types::Variant;

    use super::*;
    use crate::descriptor::OverlayKindDescriptor;
    use crate::kinds::{alert::AlertOverlayKind, frame::FrameOverlayKind};

    fn appearance(key: &str, value: &str) -> PreviewComposition {
        compose(
            PreviewShape::Strip,
            &OverlayConfig::from([(key.to_owned(), config::text(value))]),
        )
    }

    #[test]
    fn every_offered_accent_maps_to_its_own_swatch() {
        let accents: Vec<PreviewAccent> = config::ACCENT_OPTIONS
            .iter()
            .map(|name| appearance(config::ACCENT, name).accent)
            .collect();

        for (index, accent) in accents.iter().enumerate() {
            for (other_index, other) in accents.iter().enumerate().skip(index + 1) {
                assert_ne!(
                    accent,
                    other,
                    "accents {:?} and {:?} both render the same swatch",
                    config::ACCENT_OPTIONS[index],
                    config::ACCENT_OPTIONS[other_index]
                );
            }
        }
    }

    #[test]
    fn every_offered_position_maps_to_its_own_placement() {
        let positions: Vec<PreviewPosition> = config::POSITION_OPTIONS
            .iter()
            .map(|name| appearance(config::POSITION, name).position)
            .collect();

        for (index, position) in positions.iter().enumerate() {
            for (other_index, other) in positions.iter().enumerate().skip(index + 1) {
                assert_ne!(
                    position,
                    other,
                    "positions {:?} and {:?} both render the same placement",
                    config::POSITION_OPTIONS[index],
                    config::POSITION_OPTIONS[other_index]
                );
            }
        }
    }

    #[test]
    fn appearance_values_this_build_does_not_know_fall_back_to_the_neutral_look() {
        for (label, config) in [
            ("absent", OverlayConfig::new()),
            (
                "written by a newer build",
                OverlayConfig::from([
                    (config::ACCENT.to_owned(), config::text("teal")),
                    (config::FONT.to_owned(), config::text("Comic Sans")),
                    (config::POSITION.to_owned(), config::text("diagonal")),
                ]),
            ),
            (
                "stored with the wrong type",
                OverlayConfig::from([
                    (config::ACCENT.to_owned(), Variant::Int(3)),
                    (config::FONT.to_owned(), Variant::Bool(true)),
                    (config::POSITION.to_owned(), Variant::Int(0)),
                ]),
            ),
        ] {
            let composition = compose(PreviewShape::Strip, &config);

            assert_eq!(composition.accent, PreviewAccent::Mauve, "accent {label}");
            assert_eq!(composition.font, PreviewFont::Sans, "font {label}");
            assert_eq!(
                composition.position,
                PreviewPosition::Center,
                "position {label}"
            );
        }
    }

    #[test]
    fn only_non_empty_lines_are_composed_and_the_headline_leads() {
        for (label, config, expected) in [
            (
                "both lines set",
                AlertOverlayKind.default_config(),
                vec![PreviewLineRole::Headline, PreviewLineRole::Subline],
            ),
            (
                "empty headline",
                FrameOverlayKind.default_config(),
                vec![PreviewLineRole::Subline],
            ),
            ("nothing set", OverlayConfig::new(), Vec::new()),
        ] {
            let roles: Vec<PreviewLineRole> = compose(PreviewShape::Strip, &config)
                .lines
                .iter()
                .map(|line| line.role)
                .collect();

            assert_eq!(roles, expected, "{label}");
        }
    }

    #[test]
    fn line_text_keeps_its_variable_tokens_unexpanded() {
        let config = OverlayConfig::from([(
            config::HEADLINE.to_owned(),
            config::text("%user% just subscribed!"),
        )]);

        let composition = compose(PreviewShape::Strip, &config);

        assert_eq!(
            composition.lines.first().map(|line| line.text.as_str()),
            Some("%user% just subscribed!"),
            "expanding tokens is the caller's job, not the preview's"
        );
    }
}
