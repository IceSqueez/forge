use forge_registry::{FormField, effective_config};
use forge_types::Variant;

use crate::catalog::descriptor::{OverlayConfig, OverlayKindDescriptor, SectionedField};
use crate::error::OverlayError;

pub fn effective_overlay_config(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
) -> OverlayConfig {
    effective_config(&descriptor.default_config(), stored)
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
        | FormField::DependentSelect { key, .. } => expect_string(config, key).map(|_| ()),
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
