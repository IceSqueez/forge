mod fields;
mod keys;
mod validation;

use forge_types::Variant;

use crate::catalog::descriptor::OverlayConfig;

pub(crate) use fields::*;
pub use keys::*;
pub use validation::{effective_overlay_config, hex_color_channels, validate_overlay_config};

pub(crate) fn read_str<'a>(config: &'a OverlayConfig, key: &str) -> &'a str {
    config
        .get(key)
        .and_then(Variant::as_str)
        .unwrap_or_default()
}

pub(crate) fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::validation::check_field;
    use super::*;
    use crate::catalog::descriptor::{
        ConfigSection, DeliveryDisposition, OverlayKindDescriptor, SectionedField,
    };
    use crate::error::OverlayError;
    use crate::files::assets::PageAssets;
    use crate::kinds::alert::AlertOverlayKind;
    use crate::kinds::chat::ChatOverlayKind;
    use crate::motion;
    use crate::render::preview::{PreviewComposition, PreviewShape, compose};
    use forge_registry::{AmountUnit, DurationBounds, FormField, UnitAmountBounds};
    use forge_types::MessageCode;

    const PROBE_CODE: MessageCode = MessageCode::from_static("probe", "kind");

    const PROBE_TOGGLE: &str = "probe.toggle";
    const PROBE_INTEGER: &str = "probe.integer";
    const PROBE_OPTIONAL: &str = "probe.optional";
    const PROBE_INNER: &str = "probe.inner";
    const PROBE_INNER_OPTIONS: &[&str] = &["left", "right"];
    const PROBE_AMOUNT: &str = "probe.amount";
    const PROBE_AMOUNT_UNIT: &str = "probe.amount_unit";
    const PROBE_DURATION: &str = "probe.duration";
    const PROBE_DURATION_BOUNDS: DurationBounds = DurationBounds::in_millis(100, 600_000);
    const PROBE_AMOUNT_BOUNDS: UnitAmountBounds = UnitAmountBounds {
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
    };

    struct FieldProbeKind;

    impl OverlayKindDescriptor for FieldProbeKind {
        fn id(&self) -> &str {
            "probe"
        }

        fn label(&self) -> MessageCode {
            PROBE_CODE
        }

        fn summary(&self) -> MessageCode {
            PROBE_CODE
        }

        fn icon_name(&self) -> &str {
            ""
        }

        fn delivery_disposition(&self) -> DeliveryDisposition {
            DeliveryDisposition::Transient
        }

        fn order_sensitive(&self) -> bool {
            false
        }

        fn config_schema_version(&self) -> u32 {
            1
        }

        fn default_config(&self) -> OverlayConfig {
            OverlayConfig::new()
        }

        fn config_fields(&self) -> Vec<SectionedField> {
            vec![
                in_section(
                    ConfigSection::Behavior,
                    FormField::Toggle {
                        key: PROBE_TOGGLE,
                        label: "Toggle",
                    },
                ),
                in_section(
                    ConfigSection::Behavior,
                    FormField::Integer {
                        key: PROBE_INTEGER,
                        label: "Integer",
                        min: 0,
                        max: 10,
                    },
                ),
                in_section(
                    ConfigSection::Behavior,
                    FormField::UnitAmount {
                        key: PROBE_AMOUNT,
                        label: "Amount",
                        unit_key: PROBE_AMOUNT_UNIT,
                        bounds: PROBE_AMOUNT_BOUNDS,
                    },
                ),
                in_section(
                    ConfigSection::Behavior,
                    FormField::Duration {
                        key: PROBE_DURATION,
                        label: "Duration",
                        bounds: PROBE_DURATION_BOUNDS,
                    },
                ),
                in_section(
                    ConfigSection::Behavior,
                    FormField::Optional {
                        key: PROBE_OPTIONAL,
                        label: "Optional",
                        inner: Box::new(FormField::Select {
                            key: PROBE_INNER,
                            label: "Inner",
                            options: PROBE_INNER_OPTIONS,
                        }),
                    },
                ),
            ]
        }

        fn preview(
            &self,
            config: &OverlayConfig,
            _: forge_types::NamesLanguage,
        ) -> PreviewComposition {
            compose(PreviewShape::Strip, None, config)
        }
        fn page_assets(&self) -> PageAssets {
            PageAssets {
                markup: "",
                style: "",
                behavior: "",
            }
        }
    }

    fn one(key: &str, value: Variant) -> OverlayConfig {
        OverlayConfig::from([(key.to_owned(), value)])
    }

    #[test]
    fn effective_config_layers_stored_over_defaults_and_keeps_unknown_keys() {
        let descriptor = AlertOverlayKind;
        let defaults = descriptor.default_config();
        let stored = OverlayConfig::from([
            (HEADLINE.to_owned(), text("Custom headline")),
            ("vendor.future_key".to_owned(), Variant::Bool(true)),
        ]);

        let effective = effective_overlay_config(&descriptor, &stored);

        assert_eq!(
            effective.get(HEADLINE),
            Some(&text("Custom headline")),
            "the stored value must win over the kind default"
        );
        assert_eq!(
            effective.get(ACCENT),
            defaults.get(ACCENT),
            "a key the stored config omits must fall back to the kind default"
        );
        assert_eq!(
            effective.get("vendor.future_key"),
            Some(&Variant::Bool(true)),
            "a key written by a newer build must survive the merge"
        );
    }

    #[test]
    fn validate_accepts_a_config_that_omits_every_declared_key() {
        validate_overlay_config(&AlertOverlayKind, &OverlayConfig::new())
            .expect("a stored config is sparse and carries no values to reject");
    }

    #[test]
    fn validate_ignores_keys_no_field_declares() {
        let config = one(
            "vendor.future_key",
            Variant::Array(vec![Variant::Int(1), Variant::Bool(false)]),
        );

        validate_overlay_config(&AlertOverlayKind, &config)
            .expect("an undeclared key must be left alone whatever it holds");
    }

    #[test]
    fn validate_rejects_a_choice_outside_the_declared_options() {
        let alert = AlertOverlayKind;
        let chat = ChatOverlayKind;

        for (descriptor, key, value) in [
            (&alert as &dyn OverlayKindDescriptor, ACCENT, "teal"),
            (&alert, FONT, "Comic Sans"),
            (&alert, motion::ENTRANCE, "shatter"),
            (&alert, motion::EXIT, "typewriter"),
            (&alert, motion::TEXT_EFFECT, "burst"),
            (&alert, motion::INTENSITY, "extreme"),
            (&chat, motion::ENTRANCE, "explode"),
            (&alert, ACCENT, ""),
            (&chat, POSITION, "diagonal"),
            (&chat, POSITION, "center"),
        ] {
            let err = validate_overlay_config(descriptor, &one(key, text(value)))
                .expect_err("an unlisted choice must be rejected");

            assert!(
                matches!(&err, OverlayError::UnknownChoice { key: k, value: v } if k == key && v == value),
                "{key} = {value:?} produced {err:?}"
            );
        }
    }

    #[test]
    fn validate_rejects_a_value_of_the_wrong_type() {
        let alert = AlertOverlayKind;
        let probe = FieldProbeKind;

        for (descriptor, key, value) in [
            (
                &alert as &dyn OverlayKindDescriptor,
                HEADLINE,
                Variant::Int(1),
            ),
            (&alert, ACCENT, Variant::Bool(true)),
            (&alert, FONT, Variant::Array(Vec::new())),
            (&alert, DURATION, text("5")),
            (&alert, DURATION, Variant::Float(5.0)),
            (&probe, PROBE_TOGGLE, text("yes")),
            (&probe, PROBE_INTEGER, Variant::Bool(true)),
            (&probe, PROBE_OPTIONAL, Variant::Int(1)),
            (&probe, PROBE_AMOUNT, text("2")),
            (&probe, PROBE_AMOUNT_UNIT, Variant::Int(60)),
            (&probe, PROBE_DURATION, text("2000")),
        ] {
            let err = validate_overlay_config(descriptor, &one(key, value.clone()))
                .expect_err("a value of the wrong shape must be rejected");

            assert!(
                matches!(&err, OverlayError::WrongType { key: k, .. } if k == key),
                "{key} = {value:?} produced {err:?}"
            );
        }
    }

    #[test]
    fn validate_accepts_the_duration_bounds_and_rejects_one_step_past_each() {
        for accepted in [
            DURATION_MIN_SECS,
            DURATION_MIN_SECS + 1,
            DURATION_MAX_SECS - 1,
            DURATION_MAX_SECS,
        ] {
            validate_overlay_config(&AlertOverlayKind, &one(DURATION, Variant::Int(accepted)))
                .unwrap_or_else(|e| panic!("duration {accepted} sits in range but produced {e:?}"));
        }

        for rejected in [
            DURATION_MIN_SECS - 1,
            DURATION_MAX_SECS + 1,
            i64::MIN,
            i64::MAX,
        ] {
            let err =
                validate_overlay_config(&AlertOverlayKind, &one(DURATION, Variant::Int(rejected)))
                    .expect_err("a duration outside the slider range must be rejected");

            assert!(
                matches!(&err, OverlayError::OutOfRange { key, .. } if key == DURATION),
                "duration {rejected} produced {err:?}"
            );
        }
    }

    fn amount_in(amount: i64, unit: Option<&str>) -> OverlayConfig {
        let mut config = one(PROBE_AMOUNT, Variant::Int(amount));
        if let Some(unit) = unit {
            config.insert(PROBE_AMOUNT_UNIT.to_owned(), text(unit));
        }
        config
    }

    #[test]
    fn validate_accepts_a_unit_amount_up_to_the_bound_of_its_stored_unit() {
        for (amount, unit) in [
            (120, Some("seconds")),
            (1, Some("seconds")),
            (2, Some("minutes")),
            (1, Some("minutes")),
            (120, None),
            (120, Some("hours")),
        ] {
            validate_overlay_config(&FieldProbeKind, &amount_in(amount, unit))
                .unwrap_or_else(|e| panic!("{amount} {unit:?} sits in range but produced {e:?}"));
        }
    }

    #[test]
    fn validate_rejects_a_unit_amount_past_the_bound_of_its_stored_unit_naming_that_bound() {
        for (amount, unit, expected_max) in [
            (121, Some("seconds"), 120),
            (3, Some("minutes"), 2),
            (0, Some("minutes"), 2),
            (121, None, 120),
            (121, Some("hours"), 120),
        ] {
            let err = validate_overlay_config(&FieldProbeKind, &amount_in(amount, unit))
                .expect_err("an amount outside its unit's range must be rejected");

            assert!(
                matches!(
                    &err,
                    OverlayError::OutOfRange { key, min: 1, max } if key == PROBE_AMOUNT && *max == expected_max
                ),
                "{amount} {unit:?} produced {err:?}"
            );
        }
    }

    #[test]
    fn validate_checks_a_duration_against_its_millisecond_bounds() {
        for (millis, accepted) in [
            (100, true),
            (600_000, true),
            (99, false),
            (600_001, false),
            (i64::MIN, false),
        ] {
            let rejection = validate_overlay_config(
                &FieldProbeKind,
                &one(PROBE_DURATION, Variant::Int(millis)),
            )
            .err()
            .map(|err| {
                matches!(
                    &err,
                    OverlayError::OutOfRange { key, min: 100, max: 600_000 } if key == PROBE_DURATION
                )
            });

            assert_eq!(rejection, (!accepted).then_some(true), "{millis} ms");
        }
    }

    #[test]
    fn validate_walks_into_the_field_an_optional_wraps() {
        let config = OverlayConfig::from([
            (PROBE_OPTIONAL.to_owned(), Variant::Bool(true)),
            (PROBE_INNER.to_owned(), text("sideways")),
        ]);

        let err = validate_overlay_config(&FieldProbeKind, &config)
            .expect_err("the wrapped field keeps its own constraints");

        assert!(
            matches!(&err, OverlayError::UnknownChoice { key, .. } if key == PROBE_INNER),
            "the inner field was not validated: {err:?}"
        );
    }

    #[test]
    fn a_one_based_field_bounds_the_stored_value_from_zero_to_one_below_the_count() {
        let field = FormField::OneBasedInteger {
            key: PROBE_INTEGER,
            label: "Channel",
            count: 16,
        };

        for (stored, accepted) in [(-1, false), (0, true), (15, true), (16, false)] {
            let config = one(PROBE_INTEGER, Variant::Int(stored));

            assert_eq!(
                check_field(&field, &config).is_ok(),
                accepted,
                "stored {stored}"
            );
        }
    }
}
