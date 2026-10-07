use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguage {
    Rhai,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmountUnit {
    pub value: &'static str,
    pub base_units: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitAmountBounds {
    pub min: i64,
    pub max_base_units: u64,
    pub units: &'static [AmountUnit],
}

impl UnitAmountBounds {
    pub fn unit_or_first(&self, value: &str) -> Option<&'static AmountUnit> {
        self.units
            .iter()
            .find(|unit| unit.value == value)
            .or_else(|| self.units.first())
    }

    pub fn range_for(&self, unit: &str) -> RangeInclusive<i64> {
        let base_units = self
            .unit_or_first(unit)
            .map_or(1, |unit| unit.base_units.max(1));
        let max = i64::try_from(self.max_base_units / base_units).unwrap_or(i64::MAX);
        self.min..=max.max(self.min)
    }
}

#[derive(Debug, Clone)]
pub enum FormField {
    Text {
        key: &'static str,
        label: &'static str,
        placeholder: &'static str,
    },
    TextArea {
        key: &'static str,
        label: &'static str,
    },
    Code {
        key: &'static str,
        label: &'static str,
        language: CodeLanguage,
    },
    Integer {
        key: &'static str,
        label: &'static str,
        min: i64,
        max: i64,
    },
    Slider {
        key: &'static str,
        label: &'static str,
        min: i64,
        max: i64,
        unit: &'static str,
    },
    Toggle {
        key: &'static str,
        label: &'static str,
    },
    FilePicker {
        key: &'static str,
        label: &'static str,
    },
    DateTime {
        key: &'static str,
        label: &'static str,
    },
    Select {
        key: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    },
    UnitAmount {
        key: &'static str,
        label: &'static str,
        unit_key: &'static str,
        bounds: UnitAmountBounds,
    },
    DynamicSelect {
        key: &'static str,
        label: &'static str,
        options_key: &'static str,
    },
    DependentSelect {
        key: &'static str,
        label: &'static str,
        options_prefix: &'static str,
        depends_on: &'static str,
    },
    Swatch {
        key: &'static str,
        label: &'static str,
        options: &'static [&'static str],
    },
    Optional {
        key: &'static str,
        label: &'static str,
        inner: Box<FormField>,
    },
    SubChain {
        key: &'static str,
        label: &'static str,
    },
    CaseList {
        key: &'static str,
        label: &'static str,
    },
}

impl FormField {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Text { key, .. }
            | Self::TextArea { key, .. }
            | Self::Code { key, .. }
            | Self::Integer { key, .. }
            | Self::Slider { key, .. }
            | Self::Toggle { key, .. }
            | Self::FilePicker { key, .. }
            | Self::DateTime { key, .. }
            | Self::Select { key, .. }
            | Self::UnitAmount { key, .. }
            | Self::DynamicSelect { key, .. }
            | Self::DependentSelect { key, .. }
            | Self::Swatch { key, .. }
            | Self::Optional { key, .. }
            | Self::SubChain { key, .. }
            | Self::CaseList { key, .. } => key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECONDS_PER_DAY: u64 = 86_400;

    const CLOCK_BOUNDS: UnitAmountBounds = UnitAmountBounds {
        min: 1,
        max_base_units: 365 * SECONDS_PER_DAY,
        units: &[
            AmountUnit {
                value: "minutes",
                base_units: 60,
            },
            AmountUnit {
                value: "hours",
                base_units: 3_600,
            },
            AmountUnit {
                value: "days",
                base_units: SECONDS_PER_DAY,
            },
        ],
    };

    fn one_unit(value: &'static str, base_units: u64) -> &'static [AmountUnit] {
        Box::leak(Box::new([AmountUnit { value, base_units }]))
    }

    #[test]
    fn range_for_caps_each_unit_at_the_shared_total_expressed_in_that_unit() {
        for (unit, expected) in [
            ("minutes", 1..=525_600),
            ("hours", 1..=8_760),
            ("days", 1..=365),
        ] {
            assert_eq!(CLOCK_BOUNDS.range_for(unit), expected, "unit {unit}");
        }
    }

    #[test]
    fn range_for_an_unlisted_unit_uses_the_first_unit() {
        for unit in ["weeks", "", "Days", " hours"] {
            assert_eq!(
                CLOCK_BOUNDS.range_for(unit),
                CLOCK_BOUNDS.range_for("minutes"),
                "unit {unit:?} is not listed and must read as the first unit"
            );
        }
    }

    #[test]
    fn unit_or_first_finds_the_named_unit_and_falls_back_to_the_first() {
        let resolved = ["hours", "fortnights"]
            .map(|unit| CLOCK_BOUNDS.unit_or_first(unit).map(|unit| unit.value));

        assert_eq!(resolved, [Some("hours"), Some("minutes")]);
    }

    #[test]
    fn range_for_rounds_a_total_the_unit_does_not_divide_down_to_whole_units() {
        let bounds = UnitAmountBounds {
            min: 1,
            max_base_units: 100,
            units: one_unit("thirties", 30),
        };

        assert_eq!(bounds.range_for("thirties"), 1..=3);
    }

    #[test]
    fn range_for_never_drops_its_end_below_the_minimum() {
        for (max_base_units, base_units, expected) in [
            (59, 60, 1..=1),
            (60, 60, 1..=1),
            (120, 60, 1..=2),
            (0, 60, 1..=1),
        ] {
            let bounds = UnitAmountBounds {
                min: 1,
                max_base_units,
                units: one_unit("minutes", base_units),
            };
            assert_eq!(
                bounds.range_for("minutes"),
                expected,
                "{max_base_units} base units at {base_units} per unit"
            );
        }
    }

    #[test]
    fn range_for_a_zero_sized_unit_counts_it_as_one_base_unit_instead_of_dividing_by_zero() {
        let bounds = UnitAmountBounds {
            min: 0,
            max_base_units: 500,
            units: one_unit("ticks", 0),
        };

        assert_eq!(bounds.range_for("ticks"), 0..=500);
    }

    #[test]
    fn range_for_with_no_units_spans_the_whole_total_and_finds_no_unit() {
        let bounds = UnitAmountBounds {
            min: 1,
            max_base_units: 90,
            units: &[],
        };

        assert_eq!(
            (bounds.range_for("minutes"), bounds.unit_or_first("minutes")),
            (1..=90, None)
        );
    }

    #[test]
    fn range_for_a_total_beyond_i64_saturates_at_i64_max() {
        let bounds = UnitAmountBounds {
            min: 1,
            max_base_units: u64::MAX,
            units: one_unit("units", 1),
        };

        assert_eq!(*bounds.range_for("units").end(), i64::MAX);
    }

    #[test]
    fn key_names_the_config_key_each_field_shape_writes_to() {
        let cases = vec![
            (
                FormField::Text {
                    key: "alias_name",
                    label: "Alias name",
                    placeholder: "e.g. %user%",
                },
                "alias_name",
            ),
            (
                FormField::Code {
                    key: "body",
                    label: "Script",
                    language: CodeLanguage::Rhai,
                },
                "body",
            ),
            (
                FormField::Slider {
                    key: "volume_db",
                    label: "Volume",
                    min: -60,
                    max: 6,
                    unit: "dB",
                },
                "volume_db",
            ),
            (
                FormField::Select {
                    key: "mode",
                    label: "Mode",
                    options: &["first", "last"],
                },
                "mode",
            ),
            (
                FormField::DynamicSelect {
                    key: "engine_id",
                    label: "Engine ID",
                    options_key: "tts.engine_ids",
                },
                "engine_id",
            ),
            (
                FormField::DependentSelect {
                    key: "voice_id",
                    label: "Voice ID",
                    options_prefix: "tts.voices",
                    depends_on: "engine_id",
                },
                "voice_id",
            ),
            (
                FormField::Swatch {
                    key: "accent",
                    label: "Accent",
                    options: &["mauve"],
                },
                "accent",
            ),
            (
                FormField::UnitAmount {
                    key: "delay_amount",
                    label: "Run after",
                    unit_key: "delay_unit",
                    bounds: CLOCK_BOUNDS,
                },
                "delay_amount",
            ),
            (
                FormField::Optional {
                    key: "use_voice",
                    label: "Override voice",
                    inner: Box::new(FormField::Text {
                        key: "voice_id",
                        label: "Voice ID",
                        placeholder: "",
                    }),
                },
                "use_voice",
            ),
        ];

        for (field, expected) in cases {
            assert_eq!(
                field.key(),
                expected,
                "{field:?} must name its own config key, not a sibling string field"
            );
        }
    }
}
