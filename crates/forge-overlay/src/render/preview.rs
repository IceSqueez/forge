use forge_types::{LATEST_DONATION_SLOT, NOW_PLAYING_SLOT, Variant};

use crate::catalog::descriptor::OverlayConfig;
use crate::catalog::source_box::{ContentMargins, DesignSize};
use crate::config;
use crate::metrics::{ALERT, BRIGHT_INK_RGB, ElementSizing, SURFACE_RGB};

const PERCENT_SCALE: f32 = 100.0;
pub const TALLY_SEPARATOR: &str = " / ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewShape {
    BadgeBanner,
    Blank,
    BorderedFrame,
    LatestCard,
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
    Custom([u8; 3]),
}

impl PreviewAccent {
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::Mauve => Some(config::ACCENT_MAUVE),
            Self::Sky => Some(config::ACCENT_SKY),
            Self::Green => Some(config::ACCENT_GREEN),
            Self::Peach => Some(config::ACCENT_PEACH),
            Self::Yellow => Some(config::ACCENT_YELLOW),
            Self::Red => Some(config::ACCENT_RED),
            Self::Custom(_) => None,
        }
    }

    pub fn from_stored(stored: &str) -> Self {
        if let Some(channels) = config::hex_color_channels(stored) {
            return Self::Custom(channels);
        }
        match stored {
            config::ACCENT_SKY => Self::Sky,
            config::ACCENT_GREEN => Self::Green,
            config::ACCENT_PEACH => Self::Peach,
            config::ACCENT_YELLOW => Self::Yellow,
            config::ACCENT_RED => Self::Red,
            _ => Self::Mauve,
        }
    }

    pub fn channels(self) -> [u8; 3] {
        let hex = match self {
            Self::Custom(channels) => return channels,
            Self::Mauve => config::ACCENT_MAUVE_HEX,
            Self::Sky => config::ACCENT_SKY_HEX,
            Self::Green => config::ACCENT_GREEN_HEX,
            Self::Peach => config::ACCENT_PEACH_HEX,
            Self::Yellow => config::ACCENT_YELLOW_HEX,
            Self::Red => config::ACCENT_RED_HEX,
        };
        config::hex_color_channels(hex).unwrap_or_default()
    }
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
    Label,
    Headline,
    Subline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMark {
    Donation,
    NowPlaying,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewSurface {
    pub rgb: [u8; 3],
    pub alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewImageSide {
    Above,
    Below,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewImage {
    pub side: PreviewImageSide,
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewTextOverflow {
    Wrap,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewCard {
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
    pub overflow: PreviewTextOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewLabelSide {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewLabel {
    pub side: PreviewLabelSide,
    pub offset_x: i16,
    pub offset_y: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLine {
    pub role: PreviewLineRole,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewCanvas {
    pub width: u32,
    pub height: u32,
}

impl From<DesignSize> for PreviewCanvas {
    fn from(size: DesignSize) -> Self {
        Self {
            width: size.width,
            height: size.height,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewElement {
    pub text_size: Option<u32>,
    pub sizing: Option<ElementSizing>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewComposition {
    pub shape: PreviewShape,
    pub main_text_color: PreviewAccent,
    pub text_color: PreviewAccent,
    pub border_color: PreviewAccent,
    pub font: PreviewFont,
    pub position: PreviewPosition,
    pub canvas: PreviewCanvas,
    pub margins: ContentMargins,
    pub element: PreviewElement,
    pub lines: Vec<PreviewLine>,
    pub fill: Option<f32>,
    pub mark: Option<PreviewMark>,
    pub surface: Option<PreviewSurface>,
    pub image: Option<PreviewImage>,
    pub card: Option<PreviewCard>,
    pub label: Option<PreviewLabel>,
    pub author_ink: Option<[u8; 3]>,
}

pub(crate) fn compose(
    shape: PreviewShape,
    sizing: Option<ElementSizing>,
    config: &OverlayConfig,
) -> PreviewComposition {
    PreviewComposition {
        shape,
        main_text_color: main_text_color_of(config),
        text_color: color_of(config, config::TEXT_COLOR),
        border_color: color_of(config, config::BORDER_COLOR),
        font: font_of(config),
        position: position_of(shape, config),
        canvas: DesignSize::read(config, DesignSize::BROWSER_SOURCE_DEFAULT).into(),
        margins: ContentMargins::read(config, ContentMargins::NONE),
        element: element_of(sizing, config),
        lines: lines_of(shape, config),
        fill: fill_of(shape, config),
        mark: mark_of(shape, config),
        surface: surface_of(shape, config),
        image: image_of(shape, config),
        card: card_of(shape, config),
        label: label_of(shape, config),
        author_ink: author_ink_of(shape, config),
    }
}

fn author_ink_of(shape: PreviewShape, config: &OverlayConfig) -> Option<[u8; 3]> {
    if shape != PreviewShape::MessageFeed {
        return None;
    }
    config::hex_color_channels(config::read_str(config, config::AUTHOR_COLOR).trim())
}

fn label_of(shape: PreviewShape, config: &OverlayConfig) -> Option<PreviewLabel> {
    if shape != PreviewShape::BorderedFrame {
        return None;
    }
    let side = match config::read_str(config, config::LABEL_SIDE) {
        config::LABEL_SIDE_CENTER => PreviewLabelSide::Center,
        config::LABEL_SIDE_RIGHT => PreviewLabelSide::Right,
        _ => PreviewLabelSide::Left,
    };
    Some(PreviewLabel {
        side,
        offset_x: label_offset(config, config::LABEL_OFFSET_X),
        offset_y: label_offset(config, config::LABEL_OFFSET_Y),
    })
}

fn label_offset(config: &OverlayConfig, key: &str) -> i16 {
    config
        .get(key)
        .and_then(Variant::as_int)
        .map(|offset| offset.clamp(config::LABEL_OFFSET_MIN_PX, config::LABEL_OFFSET_MAX_PX))
        .and_then(|offset| i16::try_from(offset).ok())
        .unwrap_or_default()
}

fn surface_of(shape: PreviewShape, config: &OverlayConfig) -> Option<PreviewSurface> {
    if shape != PreviewShape::BadgeBanner {
        return None;
    }
    let percent = config
        .get(config::BACKGROUND_OPACITY)
        .and_then(Variant::as_int)
        .map(|percent| {
            percent.clamp(
                config::BACKGROUND_OPACITY_MIN_PERCENT,
                config::BACKGROUND_OPACITY_MAX_PERCENT,
            )
        })
        .and_then(|percent| u8::try_from(percent).ok());
    Some(PreviewSurface {
        rgb: config::hex_color_channels(config::read_str(config, config::BACKGROUND_COLOR))
            .unwrap_or(SURFACE_RGB),
        alpha: percent.map_or(ALERT.surface_alpha, |percent| {
            f32::from(percent) / PERCENT_SCALE
        }),
    })
}

fn image_of(shape: PreviewShape, config: &OverlayConfig) -> Option<PreviewImage> {
    if shape != PreviewShape::BadgeBanner || config::read_str(config, config::IMAGE).is_empty() {
        return None;
    }
    let side = match config::read_str(config, config::IMAGE_POSITION) {
        config::IMAGE_POSITION_BELOW => PreviewImageSide::Below,
        config::IMAGE_POSITION_LEFT => PreviewImageSide::Left,
        config::IMAGE_POSITION_RIGHT => PreviewImageSide::Right,
        _ => PreviewImageSide::Above,
    };
    Some(PreviewImage {
        side,
        height: u16::try_from(bounded_px(
            config,
            config::IMAGE_HEIGHT,
            config::IMAGE_HEIGHT_MIN_PX,
            config::IMAGE_HEIGHT_MAX_PX,
        )?)
        .ok()?,
    })
}

fn card_of(shape: PreviewShape, config: &OverlayConfig) -> Option<PreviewCard> {
    if shape != PreviewShape::BadgeBanner {
        return None;
    }
    let least = |key| {
        bounded_px(
            config,
            key,
            config::CARD_MIN_SIZE_MIN_PX,
            config::CARD_SIZE_MAX_PX,
        )
        .unwrap_or_default()
    };
    let most = |toggle, key| {
        (config.get(toggle).and_then(Variant::as_bool) == Some(true))
            .then(|| {
                bounded_px(
                    config,
                    key,
                    config::CARD_MAX_SIZE_MIN_PX,
                    config::CARD_SIZE_MAX_PX,
                )
            })
            .flatten()
    };
    Some(PreviewCard {
        min_width: least(config::CARD_MIN_WIDTH),
        min_height: least(config::CARD_MIN_HEIGHT),
        max_width: most(config::CARD_WIDTH_LIMITED, config::CARD_MAX_WIDTH),
        max_height: most(config::CARD_HEIGHT_LIMITED, config::CARD_MAX_HEIGHT),
        overflow: match config::read_str(config, config::TEXT_OVERFLOW) {
            config::TEXT_OVERFLOW_ELLIPSIS => PreviewTextOverflow::Ellipsis,
            _ => PreviewTextOverflow::Wrap,
        },
    })
}

fn main_text_color_of(config: &OverlayConfig) -> PreviewAccent {
    let stored = config::read_str(config, config::MAIN_TEXT_COLOR);
    PreviewAccent::Custom(config::hex_color_channels(stored).unwrap_or(BRIGHT_INK_RGB))
}

fn color_of(config: &OverlayConfig, key: &str) -> PreviewAccent {
    PreviewAccent::from_stored(config::read_str(config, key))
}

fn font_of(config: &OverlayConfig) -> PreviewFont {
    match config::read_str(config, config::FONT) {
        config::FONT_JETBRAINS_MONO => PreviewFont::Mono,
        _ => PreviewFont::Sans,
    }
}

fn position_of(shape: PreviewShape, config: &OverlayConfig) -> PreviewPosition {
    if !matches!(
        shape,
        PreviewShape::MessageFeed | PreviewShape::BorderedFrame
    ) {
        return PreviewPosition::Center;
    }
    if config::read_str(config, config::POSITION) == config::POSITION_TOP {
        PreviewPosition::Top
    } else {
        PreviewPosition::Bottom
    }
}

fn element_of(sizing: Option<ElementSizing>, config: &OverlayConfig) -> PreviewElement {
    let Some(sizing) = sizing else {
        return PreviewElement {
            text_size: None,
            sizing: None,
        };
    };

    PreviewElement {
        sizing: Some(sizing),
        text_size: Some(
            bounded_px(
                config,
                config::TEXT_SIZE,
                config::TEXT_SIZE_MIN_PX,
                config::TEXT_SIZE_MAX_PX,
            )
            .unwrap_or_else(|| sizing.default_text_size_px()),
        ),
    }
}

fn bounded_px(config: &OverlayConfig, key: &str, min: i64, max: i64) -> Option<u32> {
    let stored = config.get(key).and_then(Variant::as_int)?.clamp(min, max);
    u32::try_from(stored).ok()
}

fn lines_of(shape: PreviewShape, config: &OverlayConfig) -> Vec<PreviewLine> {
    match shape {
        PreviewShape::Blank => Vec::new(),
        PreviewShape::MessageFeed => message_lines(config),
        PreviewShape::ProgressBar => progress_lines(config),
        PreviewShape::LatestCard => latest_lines(config),
        _ => paired_lines(config, config::HEADLINE, config::SUBLINE),
    }
}

fn message_lines(config: &OverlayConfig) -> Vec<PreviewLine> {
    let badges = line(
        PreviewLineRole::Label,
        config::read_str(config, config::BADGES),
    );
    badges
        .into_iter()
        .chain(paired_lines(config, config::AUTHOR, config::MESSAGE))
        .collect()
}

fn latest_lines(config: &OverlayConfig) -> Vec<PreviewLine> {
    let label = line(
        PreviewLineRole::Label,
        config::read_str(config, config::LABEL),
    );
    label
        .into_iter()
        .chain(paired_lines(config, config::HEADLINE, config::SUBLINE))
        .collect()
}

fn mark_of(shape: PreviewShape, config: &OverlayConfig) -> Option<PreviewMark> {
    if shape != PreviewShape::LatestCard {
        return None;
    }
    match config::read_str(config, config::SLOT) {
        LATEST_DONATION_SLOT => Some(PreviewMark::Donation),
        NOW_PLAYING_SLOT => Some(PreviewMark::NowPlaying),
        _ => None,
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
        format!("{value}{TALLY_SEPARATOR}{target}")
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
mod tests {
    use forge_types::Variant;

    use super::*;
    use crate::catalog::descriptor::OverlayKindDescriptor;
    use crate::kinds::{alert::AlertOverlayKind, frame::FrameOverlayKind};

    fn one_text(key: &str, value: &str) -> OverlayConfig {
        OverlayConfig::from([(key.to_owned(), config::text(value))])
    }

    fn appearance(key: &str, value: &str) -> PreviewComposition {
        compose(
            PreviewShape::Strip,
            None,
            &OverlayConfig::from([(key.to_owned(), config::text(value))]),
        )
    }

    #[test]
    fn every_offered_accent_maps_to_its_own_swatch() {
        let accents: Vec<PreviewAccent> = config::ACCENT_OPTIONS
            .iter()
            .map(|name| appearance(config::TEXT_COLOR, name).text_color)
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
    fn text_and_border_colors_are_read_from_their_own_keys() {
        let composition = compose(
            PreviewShape::Strip,
            None,
            &OverlayConfig::from([
                (config::TEXT_COLOR.to_owned(), config::text("red")),
                (config::BORDER_COLOR.to_owned(), config::text("sky")),
            ]),
        );

        assert_eq!(
            (composition.text_color, composition.border_color),
            (PreviewAccent::Red, PreviewAccent::Sky)
        );
    }

    #[test]
    fn a_look_that_keeps_position_places_each_offered_edge_on_its_own_side() {
        for shape in [PreviewShape::MessageFeed, PreviewShape::BorderedFrame] {
            for (stored, expected) in [
                (config::POSITION_TOP, PreviewPosition::Top),
                (config::POSITION_BOTTOM, PreviewPosition::Bottom),
            ] {
                let config =
                    OverlayConfig::from([(config::POSITION.to_owned(), config::text(stored))]);

                assert_eq!(
                    compose(shape, None, &config).position,
                    expected,
                    "{shape:?} placed {stored:?} on the wrong edge"
                );
            }
        }
    }

    #[test]
    fn a_look_that_keeps_position_falls_back_to_the_bottom_edge_for_anything_else() {
        for shape in [PreviewShape::MessageFeed, PreviewShape::BorderedFrame] {
            for (label, config) in [
                ("absent", OverlayConfig::new()),
                ("the retired center", one_text(config::POSITION, "center")),
                (
                    "written by a newer build",
                    one_text(config::POSITION, "diagonal"),
                ),
                (
                    "stored with the wrong type",
                    OverlayConfig::from([(config::POSITION.to_owned(), Variant::Int(0))]),
                ),
            ] {
                assert_eq!(
                    compose(shape, None, &config).position,
                    PreviewPosition::Bottom,
                    "{shape:?} with a position {label}"
                );
            }
        }
    }

    #[test]
    fn a_look_without_position_centers_whatever_position_an_older_record_still_holds() {
        for shape in [
            PreviewShape::BadgeBanner,
            PreviewShape::Strip,
            PreviewShape::ProgressBar,
        ] {
            for stored in [config::POSITION_TOP, config::POSITION_BOTTOM, "center"] {
                assert_eq!(
                    compose(shape, None, &one_text(config::POSITION, stored)).position,
                    PreviewPosition::Center,
                    "{shape:?} still placed itself by a stored {stored:?}"
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
                    (config::TEXT_COLOR.to_owned(), config::text("teal")),
                    (config::BORDER_COLOR.to_owned(), config::text("teal")),
                    (config::FONT.to_owned(), config::text("Comic Sans")),
                ]),
            ),
            (
                "stored with the wrong type",
                OverlayConfig::from([
                    (config::TEXT_COLOR.to_owned(), Variant::Int(3)),
                    (config::BORDER_COLOR.to_owned(), Variant::Int(3)),
                    (config::FONT.to_owned(), Variant::Bool(true)),
                ]),
            ),
        ] {
            let composition = compose(PreviewShape::Strip, None, &config);

            assert_eq!(
                (composition.text_color, composition.border_color),
                (PreviewAccent::Mauve, PreviewAccent::Mauve),
                "colors {label}"
            );
            assert_eq!(composition.font, PreviewFont::Sans, "font {label}");
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
            let roles: Vec<PreviewLineRole> = compose(PreviewShape::Strip, None, &config)
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
        compose(shape, None, config)
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
                compose(PreviewShape::ProgressBar, None, &config).fill,
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
                compose(shape, None, &config).fill,
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

        let composition = compose(PreviewShape::Strip, None, &config);

        assert_eq!(
            composition.lines.first().map(|line| line.text.as_str()),
            Some("%user% just subscribed!"),
            "expanding tokens is the caller's job, not the preview's"
        );
    }

    #[test]
    fn a_message_feed_carries_the_name_color_the_page_would_set_on_the_author() {
        for (stored, expected) in [
            ("#102030", Some([0x10, 0x20, 0x30])),
            (" #102030 ", Some([0x10, 0x20, 0x30])),
            ("", None),
            ("%user_color%", None),
        ] {
            let config = one_text(config::AUTHOR_COLOR, stored);

            assert_eq!(
                compose(PreviewShape::MessageFeed, None, &config).author_ink,
                expected,
                "{stored:?}"
            );
            assert_eq!(compose(PreviewShape::Strip, None, &config).author_ink, None);
        }
    }

    #[test]
    fn a_message_feed_composes_its_badges_as_the_label_line() {
        let config = OverlayConfig::from([
            (config::BADGES.to_owned(), config::text("mod")),
            (config::AUTHOR.to_owned(), config::text("PixelPal")),
            (config::MESSAGE.to_owned(), config::text("hey")),
        ]);

        let lines: Vec<(PreviewLineRole, String)> =
            compose(PreviewShape::MessageFeed, None, &config)
                .lines
                .into_iter()
                .map(|line| (line.role, line.text))
                .collect();

        assert_eq!(
            lines,
            vec![
                (PreviewLineRole::Label, "mod".to_owned()),
                (PreviewLineRole::Headline, "PixelPal".to_owned()),
                (PreviewLineRole::Subline, "hey".to_owned()),
            ]
        );
    }
}
