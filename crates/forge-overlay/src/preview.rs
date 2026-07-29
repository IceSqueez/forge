use crate::config;
use crate::descriptor::OverlayConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewShape {
    BadgeBanner,
    BorderedFrame,
    MessageFeed,
    ProgressBar,
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
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewComposition {
    pub shape: PreviewShape,
    pub accent: PreviewAccent,
    pub font: PreviewFont,
    pub position: PreviewPosition,
    pub lines: Vec<PreviewLine>,
    /// Filled share of a progress track, unset for a shape without one and for values that are
    /// not two numbers.
    pub fill: Option<f32>,
}

pub(crate) fn compose(shape: PreviewShape, config: &OverlayConfig) -> PreviewComposition {
    PreviewComposition {
        shape,
        accent: accent_of(config),
        font: font_of(config),
        position: position_of(config),
        lines: lines_of(shape, config),
        fill: fill_of(shape, config),
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

/// A kind that names its own content keys reads them here; every other kind speaks the shared
/// headline and subline vocabulary.
fn lines_of(shape: PreviewShape, config: &OverlayConfig) -> Vec<PreviewLine> {
    match shape {
        PreviewShape::MessageFeed => paired_lines(config, config::AUTHOR, config::MESSAGE),
        PreviewShape::ProgressBar => progress_lines(config),
        _ => paired_lines(config, config::HEADLINE, config::SUBLINE),
    }
}

fn paired_lines(config: &OverlayConfig, headline: &str, subline: &str) -> Vec<PreviewLine> {
    [
        (PreviewLineRole::Headline, headline),
        (PreviewLineRole::Subline, subline),
    ]
    .into_iter()
    .filter_map(|(role, key)| line(role, config::read_str(config, key)))
    .collect()
}

fn progress_lines(config: &OverlayConfig) -> Vec<PreviewLine> {
    let value = config::read_str(config, config::VALUE);
    let target = config::read_str(config, config::TARGET);
    let tally = if value.is_empty() || target.is_empty() {
        String::new()
    } else {
        format!("{value} / {target}")
    };

    [
        line(
            PreviewLineRole::Headline,
            config::read_str(config, config::LABEL),
        ),
        line(PreviewLineRole::Subline, &tally),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn line(role: PreviewLineRole, text: &str) -> Option<PreviewLine> {
    (!text.is_empty()).then(|| PreviewLine {
        role,
        text: text.to_owned(),
    })
}

fn fill_of(shape: PreviewShape, config: &OverlayConfig) -> Option<f32> {
    if shape != PreviewShape::ProgressBar {
        return None;
    }
    let value: f32 = config::read_str(config, config::VALUE)
        .trim()
        .parse()
        .ok()?;
    let target: f32 = config::read_str(config, config::TARGET)
        .trim()
        .parse()
        .ok()?;
    (target > 0.0).then(|| (value / target).clamp(0.0, 1.0))
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

    fn every_vocabulary(pairs: &[(&str, &str)]) -> OverlayConfig {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), config::text(value)))
            .collect()
    }

    fn line_texts(shape: PreviewShape, config: &OverlayConfig) -> Vec<String> {
        compose(shape, config)
            .lines
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    #[test]
    fn each_shape_reads_the_content_keys_its_own_kind_declares() {
        let config = every_vocabulary(&[
            (config::HEADLINE, "Thanks for the sub!"),
            (config::SUBLINE, "Three months subscribed"),
            (config::AUTHOR, "PixelPal"),
            (config::MESSAGE, "hey chat!"),
            (config::LABEL, "Sub goal"),
            (config::VALUE, "42"),
            (config::TARGET, "100"),
        ]);

        for (shape, expected) in [
            (PreviewShape::MessageFeed, vec!["PixelPal", "hey chat!"]),
            (PreviewShape::ProgressBar, vec!["Sub goal", "42 / 100"]),
            (
                PreviewShape::BadgeBanner,
                vec!["Thanks for the sub!", "Three months subscribed"],
            ),
        ] {
            assert_eq!(
                line_texts(shape, &config),
                expected,
                "{shape:?} composed its lines from keys its kind never declares"
            );
        }
    }

    #[test]
    fn a_progress_tally_is_dropped_unless_both_of_its_numbers_are_set() {
        for (value, target, expected) in [
            ("42", "100", vec!["Sub goal", "42 / 100"]),
            ("", "100", vec!["Sub goal"]),
            ("42", "", vec!["Sub goal"]),
            ("", "", vec!["Sub goal"]),
        ] {
            let config = every_vocabulary(&[
                (config::LABEL, "Sub goal"),
                (config::VALUE, value),
                (config::TARGET, target),
            ]);

            assert_eq!(
                line_texts(PreviewShape::ProgressBar, &config),
                expected,
                "value {value:?} and target {target:?} composed a half-written tally"
            );
        }
    }

    #[test]
    fn a_progress_fill_is_a_clamped_share_and_stays_unset_when_it_cannot_be_read() {
        for (value, target, expected, label) in [
            ("25", "100", Some(0.25), "a share of the way there"),
            (
                "100",
                "100",
                Some(1.0),
                "a value sitting exactly on the target",
            ),
            ("250", "100", Some(1.0), "a value past the target"),
            ("-5", "100", Some(0.0), "a value below zero"),
            (" 42 ", "100", Some(0.42), "numbers padded with spaces"),
            ("42", "0", None, "a target of zero"),
            ("42", "-100", None, "a target below zero"),
            ("many", "100", None, "a value that is not a number"),
            ("42", "lots", None, "a target that is not a number"),
            ("", "100", None, "an unset value"),
            ("42", "", None, "an unset target"),
        ] {
            let config = every_vocabulary(&[(config::VALUE, value), (config::TARGET, target)]);

            assert_eq!(
                compose(PreviewShape::ProgressBar, &config).fill,
                expected,
                "{label} produced the wrong fill"
            );
        }
    }

    #[test]
    fn a_shape_without_a_progress_track_reports_no_fill_even_when_the_numbers_are_there() {
        let config = every_vocabulary(&[(config::VALUE, "42"), (config::TARGET, "100")]);

        for shape in [
            PreviewShape::BadgeBanner,
            PreviewShape::BorderedFrame,
            PreviewShape::MessageFeed,
            PreviewShape::Strip,
        ] {
            assert_eq!(
                compose(shape, &config).fill,
                None,
                "{shape:?} draws no track, so a fill would be rendered nowhere"
            );
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
