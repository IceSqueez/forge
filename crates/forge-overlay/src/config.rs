use forge_registry::{FormField, effective_config};
use forge_types::Variant;

use crate::descriptor::{OverlayConfig, OverlayKindDescriptor};
use crate::error::OverlayError;

pub const EVENT: &str = "event";
pub const HEADLINE: &str = "headline";
pub const SUBLINE: &str = "subline";
pub const ACCENT: &str = "accent";
pub const FONT: &str = "font";
pub const POSITION: &str = "position";
pub const ANIMATION: &str = "animation";
pub const DURATION: &str = "duration";
pub const SOUND: &str = "sound";

pub const ACCENT_OPTIONS: &[&str] = &["mauve", "sky", "green", "peach", "yellow", "red"];
pub const FONT_OPTIONS: &[&str] = &["Inter", "JetBrains Mono", "Rubik", "Bebas Neue"];
pub const POSITION_OPTIONS: &[&str] = &["top", "center", "bottom"];
pub const ANIMATION_OPTIONS: &[&str] = &[
    "fade",
    "slide-up",
    "slide-down",
    "slide-left",
    "pop",
    "wipe-left",
];

pub const EVENT_KINDS_OPTIONS_KEY: &str = "event.kinds";

pub const DURATION_MIN_SECS: i64 = 1;
pub const DURATION_MAX_SECS: i64 = 15;

pub fn effective_overlay_config(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
) -> OverlayConfig {
    effective_config(&descriptor.default_config(), stored)
}

/// Keys the descriptor does not declare are left alone, so a config written by a newer build survives.
pub fn validate_overlay_config(
    descriptor: &dyn OverlayKindDescriptor,
    config: &OverlayConfig,
) -> Result<(), OverlayError> {
    check_fields(&descriptor.config_fields(), config)
}

fn check_fields(fields: &[FormField], config: &OverlayConfig) -> Result<(), OverlayError> {
    for field in fields {
        check_field(field, config)?;
    }
    Ok(())
}

fn check_field(field: &FormField, config: &OverlayConfig) -> Result<(), OverlayError> {
    match field {
        FormField::Text { key, .. }
        | FormField::TextArea { key, .. }
        | FormField::Code { key, .. }
        | FormField::FilePicker { key, .. }
        | FormField::DateTime { key, .. }
        | FormField::DynamicSelect { key, .. } => expect_string(config, key).map(|_| ()),
        FormField::Select { key, options, .. } | FormField::Swatch { key, options, .. } => {
            let Some(value) = expect_string(config, key)? else {
                return Ok(());
            };
            if options.contains(&value) {
                return Ok(());
            }
            Err(OverlayError::UnknownChoice {
                key: (*key).to_owned(),
                value: value.to_owned(),
            })
        }
        FormField::Integer { key, min, max, .. } | FormField::Slider { key, min, max, .. } => {
            bounded_int(config, key, *min, *max)
        }
        FormField::Toggle { key, .. } => match config.get(*key) {
            None | Some(Variant::Bool(_)) => Ok(()),
            Some(_) => Err(OverlayError::WrongType {
                key: (*key).to_owned(),
                expected: "a toggle",
            }),
        },
        FormField::Optional { key, inner, .. } => match config.get(*key) {
            Some(Variant::Bool(_)) | None => check_field(inner, config),
            Some(_) => Err(OverlayError::WrongType {
                key: (*key).to_owned(),
                expected: "a toggle",
            }),
        },
        FormField::SubChain { .. } | FormField::CaseList { .. } => Ok(()),
    }
}

fn expect_string<'a>(
    config: &'a OverlayConfig,
    key: &str,
) -> Result<Option<&'a str>, OverlayError> {
    match config.get(key) {
        None => Ok(None),
        Some(Variant::String(value)) => Ok(Some(value)),
        Some(_) => Err(OverlayError::WrongType {
            key: key.to_owned(),
            expected: "text",
        }),
    }
}

fn bounded_int(config: &OverlayConfig, key: &str, min: i64, max: i64) -> Result<(), OverlayError> {
    let Some(value) = config.get(key) else {
        return Ok(());
    };
    let Variant::Int(number) = value else {
        return Err(OverlayError::WrongType {
            key: key.to_owned(),
            expected: "a whole number",
        });
    };
    if (min..=max).contains(number) {
        return Ok(());
    }
    Err(OverlayError::OutOfRange {
        key: key.to_owned(),
        min,
        max,
    })
}

pub(crate) fn read_str<'a>(config: &'a OverlayConfig, key: &str) -> &'a str {
    config
        .get(key)
        .and_then(Variant::as_str)
        .unwrap_or_default()
}

pub(crate) fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

pub(crate) fn shared_fields() -> Vec<FormField> {
    vec![
        FormField::DynamicSelect {
            key: EVENT,
            label: "On event",
            options_key: EVENT_KINDS_OPTIONS_KEY,
        },
        FormField::Text {
            key: HEADLINE,
            label: "Headline",
            placeholder: "%user% just subscribed!",
        },
        FormField::Text {
            key: SUBLINE,
            label: "Subline",
            placeholder: "Tier %tier% · %months% months",
        },
        FormField::Swatch {
            key: ACCENT,
            label: "Accent",
            options: ACCENT_OPTIONS,
        },
        FormField::Select {
            key: FONT,
            label: "Font",
            options: FONT_OPTIONS,
        },
        FormField::Select {
            key: POSITION,
            label: "Position",
            options: POSITION_OPTIONS,
        },
        FormField::Select {
            key: ANIMATION,
            label: "Animation",
            options: ANIMATION_OPTIONS,
        },
    ]
}

pub(crate) fn duration_field() -> FormField {
    FormField::Slider {
        key: DURATION,
        label: "Duration",
        min: DURATION_MIN_SECS,
        max: DURATION_MAX_SECS,
        unit: "s",
    }
}

pub(crate) fn sound_field() -> FormField {
    FormField::Text {
        key: SOUND,
        label: "Sound",
        placeholder: "fanfare.mp3",
    }
}

pub(crate) fn shared_defaults(
    accent: &str,
    font: &str,
    position: &str,
    animation: &str,
) -> OverlayConfig {
    OverlayConfig::from([
        (EVENT.to_owned(), text("")),
        (ACCENT.to_owned(), text(accent)),
        (FONT.to_owned(), text(font)),
        (POSITION.to_owned(), text(position)),
        (ANIMATION.to_owned(), text(animation)),
        (SOUND.to_owned(), text("")),
    ])
}
