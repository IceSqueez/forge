use forge_registry::{FormField, effective_config};
use forge_types::Variant;

use super::accent_split::with_split_accent;
use super::keys::{BACKGROUND_COLOR, BORDER_COLOR, IMAGE, TEXT_COLOR};
use crate::catalog::descriptor::{OverlayConfig, OverlayKindDescriptor, SectionedField};
use crate::error::OverlayError;
use crate::files::media::names_a_library_image;

const HEX_COLOR_PREFIX: char = '#';
const HEX_COLOR_DIGITS: usize = 6;
const HEX_RADIX: u32 = 16;

pub fn effective_overlay_config(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
) -> OverlayConfig {
    effective_config(&descriptor.default_config(), &with_split_accent(stored))
}

pub fn hex_color_channels(value: &str) -> Option<[u8; 3]> {
    let digits = value.strip_prefix(HEX_COLOR_PREFIX)?;
    if digits.len() != HEX_COLOR_DIGITS || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let number = u32::from_str_radix(digits, HEX_RADIX).ok()?;
    let [_, red, green, blue] = number.to_be_bytes();
    Some([red, green, blue])
}

pub fn validate_overlay_config(
    descriptor: &dyn OverlayKindDescriptor,
    config: &OverlayConfig,
) -> Result<(), OverlayError> {
    check_fields(&descriptor.config_fields(), config)
}

fn check_fields(fields: &[SectionedField], config: &OverlayConfig) -> Result<(), OverlayError> {
    for field in fields {
        check_field(&field.field, config)?;
    }
    Ok(())
}

pub(super) fn check_field(field: &FormField, config: &OverlayConfig) -> Result<(), OverlayError> {
    match field {
        FormField::Text { key, .. }
        | FormField::TextArea { key, .. }
        | FormField::Code { key, .. }
        | FormField::FilePicker { key, .. }
        | FormField::DateTime { key, .. }
        | FormField::DynamicSelect { key, .. }
        | FormField::DependentSelect { key, .. } => match expect_string(config, key)? {
            Some(value) if !is_well_formed(key, value) => Err(OverlayError::UnknownChoice {
                key: (*key).to_owned(),
                value: value.to_owned(),
            }),
            _ => Ok(()),
        },
        FormField::Select { key, options, .. } | FormField::Swatch { key, options, .. } => {
            let Some(value) = expect_string(config, key)? else {
                return Ok(());
            };
            if options.contains(&value) || is_custom_color(key, value) {
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
        FormField::OneBasedInteger { key, count, .. } => bounded_int(config, key, 0, *count - 1),
        FormField::UnitAmount {
            key,
            unit_key,
            bounds,
            ..
        } => {
            let unit = expect_string(config, unit_key)?.unwrap_or_default();
            let range = bounds.range_for(unit);
            bounded_int(config, key, *range.start(), *range.end())
        }
        FormField::Duration { key, bounds, .. } => bounded_int(config, key, bounds.min, bounds.max),
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

fn is_custom_color(key: &str, value: &str) -> bool {
    match key {
        TEXT_COLOR | BORDER_COLOR => hex_color_channels(value).is_some(),
        BACKGROUND_COLOR => value.is_empty() || hex_color_channels(value).is_some(),
        _ => false,
    }
}

fn is_well_formed(key: &str, value: &str) -> bool {
    match key {
        IMAGE => value.is_empty() || names_a_library_image(value),
        _ => true,
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
