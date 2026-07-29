use forge_registry::{FormField, effective_config};
use forge_types::Variant;

use crate::descriptor::{ConfigSection, OverlayConfig, OverlayKindDescriptor, SectionedField};
use crate::error::OverlayError;

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

fn check_fields(fields: &[SectionedField], config: &OverlayConfig) -> Result<(), OverlayError> {
    for field in fields {
        check_field(&field.field, config)?;
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

pub(crate) fn shared_fields() -> Vec<SectionedField> {
    vec![
        in_section(
            ConfigSection::Content,
            FormField::Text {
                key: HEADLINE,
                label: "Headline",
                placeholder: "Thanks for the sub!",
            },
        ),
        in_section(
            ConfigSection::Content,
            FormField::Text {
                key: SUBLINE,
                label: "Subline",
                placeholder: "Three months subscribed",
            },
        ),
        in_section(
            ConfigSection::Style,
            FormField::Swatch {
                key: ACCENT,
                label: "Accent",
                options: ACCENT_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Style,
            FormField::Select {
                key: FONT,
                label: "Font",
                options: FONT_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Style,
            FormField::Select {
                key: POSITION,
                label: "Position",
                options: POSITION_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Behavior,
            FormField::Select {
                key: ANIMATION,
                label: "Animation",
                options: ANIMATION_OPTIONS,
            },
        ),
    ]
}

pub(crate) fn duration_field() -> SectionedField {
    in_section(
        ConfigSection::Behavior,
        FormField::Slider {
            key: DURATION,
            label: "Duration",
            min: DURATION_MIN_SECS,
            max: DURATION_MAX_SECS,
            unit: "s",
        },
    )
}

pub(crate) fn sound_field() -> SectionedField {
    in_section(
        ConfigSection::Behavior,
        FormField::Text {
            key: SOUND,
            label: "Sound",
            placeholder: "fanfare.mp3",
        },
    )
}

fn in_section(section: ConfigSection, field: FormField) -> SectionedField {
    SectionedField { section, field }
}

pub(crate) fn shared_defaults(
    accent: &str,
    font: &str,
    position: &str,
    animation: &str,
) -> OverlayConfig {
    OverlayConfig::from([
        (ACCENT.to_owned(), text(accent)),
        (FONT.to_owned(), text(font)),
        (POSITION.to_owned(), text(position)),
        (ANIMATION.to_owned(), text(animation)),
        (SOUND.to_owned(), text("")),
    ])
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::assets::PageAssets;
    use crate::descriptor::DeliveryDisposition;
    use crate::kinds::alert::AlertOverlayKind;
    use crate::preview::{PreviewComposition, PreviewShape, compose};

    const PROBE_TOGGLE: &str = "probe.toggle";
    const PROBE_INTEGER: &str = "probe.integer";
    const PROBE_OPTIONAL: &str = "probe.optional";
    const PROBE_INNER: &str = "probe.inner";
    const PROBE_INNER_OPTIONS: &[&str] = &["left", "right"];

    struct FieldProbeKind;

    impl OverlayKindDescriptor for FieldProbeKind {
        fn id(&self) -> &str {
            "probe"
        }

        fn label(&self) -> &str {
            "Probe"
        }

        fn summary(&self) -> &str {
            ""
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

        fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
            compose(PreviewShape::Strip, config)
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
        for (key, value) in [
            (ACCENT, "teal"),
            (FONT, "Comic Sans"),
            (POSITION, "diagonal"),
            (ANIMATION, "explode"),
            (ACCENT, ""),
        ] {
            let err = validate_overlay_config(&AlertOverlayKind, &one(key, text(value)))
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
}
