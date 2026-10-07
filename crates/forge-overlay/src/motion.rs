use std::time::Duration;

use forge_registry::FormField;
use forge_types::Variant;

use crate::config::{read_str, text};
use crate::descriptor::{ConfigSection, OverlayConfig, SectionedField};

pub const ENTRANCE: &str = "entrance";
pub const ENTRANCE_MS: &str = "entrance_ms";
pub const TEXT_EFFECT: &str = "text_effect";
pub const TEXT_UNIT: &str = "text_unit";
pub const TEXT_STAGGER_CUSTOM: &str = "text_stagger_custom";
pub const TEXT_STAGGER_MS: &str = "text_stagger_ms";
pub const EXIT: &str = "exit";
pub const EXIT_MS: &str = "exit_ms";
pub const INTENSITY: &str = "intensity";

pub const STAGE_ID: &str = "stage";

pub const NO_MOTION: &str = "none";

pub const FADE: &str = "fade";
pub const SLIDE_UP: &str = "slide-up";
pub const SLIDE_DOWN: &str = "slide-down";
pub const SLIDE_LEFT: &str = "slide-left";
pub const SLIDE_RIGHT: &str = "slide-right";
pub const POP: &str = "pop";
pub const WIPE: &str = "wipe";
pub const SPARKS: &str = "sparks";
pub const ASSEMBLE: &str = "assemble";
pub const GLOW_BURST: &str = "glow-burst";

pub const ENTRANCE_OPTIONS: &[&str] = &[
    NO_MOTION,
    FADE,
    SLIDE_UP,
    SLIDE_DOWN,
    SLIDE_LEFT,
    SLIDE_RIGHT,
    POP,
    WIPE,
    SPARKS,
    ASSEMBLE,
    GLOW_BURST,
];

pub const TEXT_EFFECT_OPTIONS: &[&str] =
    &[NO_MOTION, "typewriter", "fly-in", "spin", "wave", "bounce"];

pub const TEXT_UNIT_LETTER: &str = "letter";
pub const TEXT_UNIT_WORD: &str = "word";
pub const TEXT_UNIT_OPTIONS: &[&str] = &[TEXT_UNIT_LETTER, TEXT_UNIT_WORD];

pub const EXIT_OPTIONS: &[&str] = &[
    NO_MOTION,
    FADE,
    SLIDE_UP,
    SLIDE_DOWN,
    SLIDE_LEFT,
    SLIDE_RIGHT,
    POP,
    WIPE,
    "dissolve",
    "smoke",
    "shatter",
    "dust",
    "burst",
];

pub const INTENSITY_MEDIUM: &str = "medium";
pub const INTENSITY_OPTIONS: &[&str] = &["low", INTENSITY_MEDIUM, "high"];

pub const MOTION_MS_MIN: i64 = 100;
pub const MOTION_MS_MAX: i64 = 3_000;
pub const DEFAULT_ENTRANCE_MS: i64 = 500;
pub const DEFAULT_EXIT_MS: i64 = 500;

pub const TEXT_STAGGER_MS_MIN: i64 = 10;
pub const TEXT_STAGGER_MS_MAX: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionAxes {
    pub entrance: bool,
    pub text_effect: bool,
    pub exit: bool,
}

impl MotionAxes {
    pub const STILL: Self = Self {
        entrance: false,
        text_effect: false,
        exit: false,
    };
    pub const ENTRANCE_ONLY: Self = Self {
        entrance: true,
        text_effect: false,
        exit: false,
    };
    pub const ENTRANCE_AND_TEXT: Self = Self {
        entrance: true,
        text_effect: true,
        exit: false,
    };
    pub const ALL: Self = Self {
        entrance: true,
        text_effect: true,
        exit: true,
    };

    pub fn any(self) -> bool {
        self.entrance || self.text_effect || self.exit
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionProfile {
    pub axes: MotionAxes,
    pub entrance: &'static str,
    pub entrance_ms: i64,
    pub exit: &'static str,
    pub exit_ms: i64,
    pub reveal_target_id: &'static str,
}

impl MotionProfile {
    pub const STILL: Self = Self {
        axes: MotionAxes::STILL,
        entrance: NO_MOTION,
        entrance_ms: DEFAULT_ENTRANCE_MS,
        exit: NO_MOTION,
        exit_ms: DEFAULT_EXIT_MS,
        reveal_target_id: STAGE_ID,
    };
}

pub(crate) fn motion_fields(axes: MotionAxes) -> Vec<SectionedField> {
    let mut fields = Vec::new();
    if axes.entrance {
        fields.push(select(ENTRANCE, "Entrance", ENTRANCE_OPTIONS));
        fields.push(milliseconds(ENTRANCE_MS, "Entrance duration"));
    }
    if axes.text_effect {
        fields.push(select(TEXT_EFFECT, "Text effect", TEXT_EFFECT_OPTIONS));
        fields.push(select(TEXT_UNIT, "Text effect by", TEXT_UNIT_OPTIONS));
        fields.push(behavior(FormField::Optional {
            key: TEXT_STAGGER_CUSTOM,
            label: "Custom text stagger",
            inner: Box::new(FormField::Slider {
                key: TEXT_STAGGER_MS,
                label: "Text stagger",
                min: TEXT_STAGGER_MS_MIN,
                max: TEXT_STAGGER_MS_MAX,
                unit: "ms",
            }),
        }));
    }
    if axes.exit {
        fields.push(select(EXIT, "Exit", EXIT_OPTIONS));
        fields.push(milliseconds(EXIT_MS, "Exit duration"));
    }
    if axes.any() {
        fields.push(select(INTENSITY, "Motion intensity", INTENSITY_OPTIONS));
    }
    fields
}

pub(crate) fn motion_defaults(profile: MotionProfile) -> OverlayConfig {
    let axes = profile.axes;
    let mut defaults = OverlayConfig::new();
    if axes.entrance {
        defaults.insert(ENTRANCE.to_owned(), text(profile.entrance));
        defaults.insert(ENTRANCE_MS.to_owned(), Variant::Int(profile.entrance_ms));
    }
    if axes.text_effect {
        defaults.insert(TEXT_EFFECT.to_owned(), text(NO_MOTION));
        defaults.insert(TEXT_UNIT.to_owned(), text(TEXT_UNIT_LETTER));
        defaults.insert(TEXT_STAGGER_CUSTOM.to_owned(), Variant::Bool(false));
    }
    if axes.exit {
        defaults.insert(EXIT.to_owned(), text(profile.exit));
        defaults.insert(EXIT_MS.to_owned(), Variant::Int(profile.exit_ms));
    }
    if axes.any() {
        defaults.insert(INTENSITY.to_owned(), text(INTENSITY_MEDIUM));
    }
    defaults
}

pub(crate) fn exit_duration(effective: &OverlayConfig) -> Duration {
    let exit = read_str(effective, EXIT);
    if exit.is_empty() || exit == NO_MOTION {
        return Duration::ZERO;
    }
    let millis = effective
        .get(EXIT_MS)
        .and_then(Variant::as_int)
        .unwrap_or(DEFAULT_EXIT_MS)
        .clamp(MOTION_MS_MIN, MOTION_MS_MAX);
    Duration::from_millis(millis.unsigned_abs())
}

fn select(
    key: &'static str,
    label: &'static str,
    options: &'static [&'static str],
) -> SectionedField {
    behavior(FormField::Select {
        key,
        label,
        options,
    })
}

fn milliseconds(key: &'static str, label: &'static str) -> SectionedField {
    behavior(FormField::Slider {
        key,
        label,
        min: MOTION_MS_MIN,
        max: MOTION_MS_MAX,
        unit: "ms",
    })
}

fn behavior(field: FormField) -> SectionedField {
    SectionedField {
        section: ConfigSection::Behavior,
        field,
    }
}
