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

#[cfg(test)]
mod tests {
    use super::*;
    use forge_registry::{AmountUnit, UnitAmountBounds};

    const KEY: &str = "timeout_ms";
    const UNIT_KEY: &str = "delay_unit";
    const POLL: AmountScale = AmountScale::Millis(DurationBounds {
        min_ms: 100,
        max_ms: 30_000,
    });
    const TIMEOUT: AmountScale = AmountScale::Millis(DurationBounds {
        min_ms: 100,
        max_ms: 600_000,
    });
    const STORED_UNIT: AmountScale = AmountScale::StoredUnit {
        unit_key: UNIT_KEY,
        bounds: UnitAmountBounds {
            min: 1,
            max_base_units: 120,
            units: &[
                AmountUnit {
                    value: "seconds",
                    base_units: 1,
                },
                AmountUnit {
                    value: "minutes",
                    base_units: 60,
                },
            ],
        },
    };

    fn stored(value: Option<Variant>) -> SubActionConfig {
        value
            .map(|value| (KEY.to_owned(), value))
            .into_iter()
            .collect()
    }

    fn shown(scale: AmountScale, value: Option<Variant>) -> (String, String) {
        let AmountSeed { unit, amount } = scale.seed(KEY, &stored(value));
        (unit, amount)
    }

    fn pair(unit: &str, amount: &str) -> (String, String) {
        (unit.to_owned(), amount.to_owned())
    }

    #[test]
    fn a_stored_duration_seeds_in_the_largest_unit_that_shows_it_exactly() {
        for (scale, millis, expected) in [
            (POLL, 500, pair(DURATION_UNIT_MILLISECONDS, "500")),
            (TIMEOUT, 30_000, pair(DURATION_UNIT_SECONDS, "30")),
            (TIMEOUT, 120_000, pair(DURATION_UNIT_MINUTES, "2")),
            (TIMEOUT, 1_500, pair(DURATION_UNIT_MILLISECONDS, "1500")),
            (TIMEOUT, 0, pair(DURATION_UNIT_MILLISECONDS, "0")),
        ] {
            assert_eq!(
                shown(scale, Some(Variant::Int(millis))),
                expected,
                "{millis} ms"
            );
        }
    }

    #[test]
    fn a_duration_stored_as_numeric_text_seeds_like_the_number() {
        assert_eq!(
            shown(TIMEOUT, Some(Variant::String(" 2000 ".to_owned()))),
            pair(DURATION_UNIT_SECONDS, "2")
        );
    }

    #[test]
    fn a_missing_or_non_numeric_duration_seeds_verbatim_in_the_first_unit() {
        for (value, amount) in [
            (None, ""),
            (Some(Variant::String("%delay%".to_owned())), "%delay%"),
        ] {
            assert_eq!(
                shown(TIMEOUT, value.clone()),
                pair(DURATION_UNIT_MILLISECONDS, amount),
                "stored {value:?}"
            );
        }
    }

    #[test]
    fn switching_a_duration_unit_converts_the_typed_amount_to_the_nearest_whole_amount() {
        for (typed, from, to, expected) in [
            (
                "2",
                DURATION_UNIT_SECONDS,
                DURATION_UNIT_MILLISECONDS,
                "2000",
            ),
            (" 3 ", DURATION_UNIT_MINUTES, DURATION_UNIT_SECONDS, "180"),
            (
                "1500",
                DURATION_UNIT_MILLISECONDS,
                DURATION_UNIT_SECONDS,
                "2",
            ),
        ] {
            assert_eq!(
                TIMEOUT.amount_after_unit_switch(typed, from, to),
                Some(expected.to_owned()),
                "{typed:?} {from} -> {to}"
            );
        }
    }

    #[test]
    fn switching_units_leaves_non_numeric_text_and_stored_unit_amounts_untouched() {
        for (scale, typed) in [(TIMEOUT, "%delay%"), (TIMEOUT, ""), (STORED_UNIT, "2")] {
            assert_eq!(
                scale.amount_after_unit_switch(typed, "seconds", "minutes"),
                None,
                "{scale:?} typed {typed:?}"
            );
        }
    }

    #[test]
    fn a_duration_stores_only_its_millis_under_its_own_key() {
        assert_eq!(
            TIMEOUT.stored_values(KEY, DURATION_UNIT_MINUTES, Some(2)),
            [(KEY.to_owned(), Variant::Int(120_000))]
        );
    }

    #[test]
    fn a_duration_with_no_parsed_amount_stores_nothing() {
        assert!(
            TIMEOUT
                .stored_values(KEY, DURATION_UNIT_SECONDS, None)
                .is_empty()
        );
    }

    #[test]
    fn a_unit_amount_stores_its_unit_and_its_amount_under_their_own_keys() {
        for (amount, expected) in [
            (
                Some(2),
                vec![
                    (UNIT_KEY.to_owned(), Variant::String("minutes".to_owned())),
                    (KEY.to_owned(), Variant::Int(2)),
                ],
            ),
            (
                None,
                vec![(UNIT_KEY.to_owned(), Variant::String("minutes".to_owned()))],
            ),
        ] {
            assert_eq!(
                STORED_UNIT.stored_values(KEY, "minutes", amount),
                expected,
                "amount {amount:?}"
            );
        }
    }

    #[test]
    fn saving_an_unedited_duration_writes_back_the_same_millis() {
        for (scale, millis) in [
            (POLL, 100),
            (POLL, 500),
            (POLL, 30_000),
            (TIMEOUT, 1_500),
            (TIMEOUT, 90_000),
            (TIMEOUT, 120_000),
            (TIMEOUT, 600_000),
            (TIMEOUT, 0),
        ] {
            let AmountSeed { unit, amount } = scale.seed(KEY, &stored(Some(Variant::Int(millis))));
            let amount = amount.parse::<i64>().ok();

            assert_eq!(
                scale.stored_values(KEY, &unit, amount),
                [(KEY.to_owned(), Variant::Int(millis))],
                "{millis} ms shown as {amount:?} {unit}"
            );
        }
    }
}
