use std::ops::RangeInclusive;

use forge_components::tr;
use forge_registry::{
    DURATION_UNIT_HOURS, DURATION_UNIT_MILLISECONDS, DURATION_UNIT_MINUTES, DURATION_UNIT_SECONDS,
    DurationBounds, FormField, UnitAmountBounds,
};
use forge_types::{SubActionConfig, Variant};

use crate::motion_labels::preset_label;

const DURATION_UNIT_PICKER_SUFFIX: &str = "#unit";

#[derive(Debug, Clone, Copy)]
pub(crate) enum AmountScale {
    StoredUnit {
        unit_key: &'static str,
        bounds: UnitAmountBounds,
    },
    Millis(DurationBounds),
}

pub(crate) struct AmountSeed {
    pub(crate) unit: String,
    pub(crate) amount: String,
}

impl AmountScale {
    pub(crate) fn of(spec: &FormField) -> Option<Self> {
        match spec {
            FormField::UnitAmount {
                unit_key, bounds, ..
            } => Some(Self::StoredUnit {
                unit_key,
                bounds: *bounds,
            }),
            FormField::Duration { bounds, .. } => Some(Self::Millis(*bounds)),
            _ => None,
        }
    }

    pub(crate) fn unit_picker_key(&self, key: &str) -> String {
        match self {
            Self::StoredUnit { unit_key, .. } => (*unit_key).to_owned(),
            Self::Millis(_) => format!("{key}{DURATION_UNIT_PICKER_SUFFIX}"),
        }
    }

    pub(crate) fn unit_options(&self) -> Vec<(String, String)> {
        match self {
            Self::StoredUnit { unit_key, bounds } => bounds
                .units
                .iter()
                .map(|unit| (unit.value.to_owned(), preset_label(unit_key, unit.value)))
                .collect(),
            Self::Millis(bounds) => bounds
                .fitting_units()
                .map(|unit| (unit.value.to_owned(), duration_unit_label(unit.value)))
                .collect(),
        }
    }

    pub(crate) fn range_for(&self, unit: &str) -> RangeInclusive<i64> {
        match self {
            Self::StoredUnit { bounds, .. } => bounds.range_for(unit),
            Self::Millis(bounds) => bounds.range_for(unit),
        }
    }

    pub(crate) fn seed(&self, key: &str, config: &SubActionConfig) -> AmountSeed {
        let stored_amount = stored_text(config, key);
        match self {
            Self::StoredUnit { unit_key, bounds } => AmountSeed {
                unit: bounds
                    .unit_or_first(&stored_text(config, unit_key))
                    .map(|unit| unit.value.to_owned())
                    .unwrap_or_default(),
                amount: stored_amount,
            },
            Self::Millis(bounds) => {
                let shown = stored_amount
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .and_then(|millis| bounds.in_largest_exact_unit(millis));
                match shown {
                    Some((unit, amount)) => AmountSeed {
                        unit: unit.value.to_owned(),
                        amount: amount.to_string(),
                    },
                    None => AmountSeed {
                        unit: bounds
                            .fitting_units()
                            .next()
                            .map(|unit| unit.value.to_owned())
                            .unwrap_or_default(),
                        amount: stored_amount,
                    },
                }
            }
        }
    }

    pub(crate) fn amount_after_unit_switch(
        &self,
        typed: &str,
        from: &str,
        to: &str,
    ) -> Option<String> {
        let Self::Millis(bounds) = self else {
            return None;
        };
        let amount = typed.trim().parse::<i64>().ok()?;
        Some(bounds.nearest_amount_in(amount, from, to).to_string())
    }

    pub(crate) fn stored_values(
        &self,
        key: &str,
        unit: &str,
        amount: Option<i64>,
    ) -> Vec<(String, Variant)> {
        match self {
            Self::StoredUnit { unit_key, .. } => {
                std::iter::once(((*unit_key).to_owned(), Variant::String(unit.to_owned())))
                    .chain(amount.map(|amount| (key.to_owned(), Variant::Int(amount))))
                    .collect()
            }
            Self::Millis(bounds) => amount
                .map(|amount| (key.to_owned(), Variant::Int(bounds.millis(unit, amount))))
                .into_iter()
                .collect(),
        }
    }

    pub(crate) fn stored_keys<'a>(&self, key: &'a str) -> Vec<&'a str> {
        match self {
            Self::StoredUnit { unit_key, .. } => vec![key, unit_key],
            Self::Millis(_) => vec![key],
        }
    }
}

fn stored_text(config: &SubActionConfig, key: &str) -> String {
    config
        .get(key)
        .map(forge_types::display_scalar)
        .unwrap_or_default()
}

fn duration_unit_label(unit: &str) -> String {
    match unit {
        DURATION_UNIT_MILLISECONDS => tr!("config_form_duration_unit_ms"),
        DURATION_UNIT_SECONDS => tr!("config_form_duration_unit_seconds"),
        DURATION_UNIT_MINUTES => tr!("config_form_duration_unit_minutes"),
        DURATION_UNIT_HOURS => tr!("config_form_duration_unit_hours"),
        _ => unit.to_owned(),
    }
}
