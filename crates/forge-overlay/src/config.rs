use forge_registry::{ENGINE_VOICE_OPTIONS_KEY, FormField, effective_config};
use forge_types::Variant;

use crate::descriptor::{ConfigSection, OverlayConfig, OverlayKindDescriptor, SectionedField};
use crate::error::OverlayError;
use crate::metrics::ElementSizing;

pub const HEADLINE: &str = "headline";
pub const SUBLINE: &str = "subline";
pub const ACCENT: &str = "accent";
pub const FONT: &str = "font";
pub const POSITION: &str = "position";
pub const ELEMENT_WIDTH: &str = "element_width";
pub const ELEMENT_HEIGHT: &str = "element_height";
pub const DESIGN_WIDTH: &str = "design_width";
pub const DESIGN_HEIGHT: &str = "design_height";
pub const MARGIN_TOP: &str = "margin_top";
pub const MARGIN_RIGHT: &str = "margin_right";
pub const MARGIN_BOTTOM: &str = "margin_bottom";
pub const MARGIN_LEFT: &str = "margin_left";
pub const MIGRATION_ACKNOWLEDGED: &str = "migration_acknowledged";
pub const TEXT_SIZE: &str = "text_size";
pub const ANIMATION: &str = "animation";
pub const DURATION: &str = "duration";
pub const SOUND: &str = "sound";
pub const SPEECH: &str = "speech";
pub const SPEECH_VOICE: &str = "speech_voice";
pub const SHOW: &str = "show";
pub const ICON: &str = "icon";

pub const AUTHOR: &str = "author";
pub const AUTHOR_COLOR: &str = "author_color";
pub const BADGES: &str = "badges";
pub const MESSAGE: &str = "message";

pub const LABEL: &str = "label";
pub const VALUE: &str = "value";
pub const TARGET: &str = "target";

pub const SLOT: &str = "slot";
pub const PLATFORM: &str = "platform";
pub const PLACEHOLDER: &str = "placeholder";

pub const CLIP_ID: &str = "clip_id";
pub const CLIP_PATH: &str = "clip_path";
pub const REPORT_PATH: &str = "report_path";
pub const CLIP_MEDIA_TYPE: &str = "clip_media_type";
pub const CLIP_DURATION_MS: &str = "clip_duration_ms";
pub const COMMAND: &str = "command";

pub const SOUND_OPTIONS_KEY: &str = "soundboard.clips";
pub const ICON_OPTIONS_KEY: &str = "overlay.icons";

pub const DEFAULT_ICON: &str = "star-filled";

pub const ACCENT_OPTIONS: &[&str] = &["mauve", "sky", "green", "peach", "yellow", "red"];
pub const FONT_OPTIONS: &[&str] = &["Inter", "JetBrains Mono", "Rubik", "Bebas Neue"];
pub const POSITION_OPTIONS: &[&str] = &["top", "bottom"];
pub const POSITION_TOP: &str = "top";
pub const POSITION_BOTTOM: &str = "bottom";
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

pub const RETIRED_KEYS: &[&str] = &[
    "canvas_width",
    "canvas_height",
    ELEMENT_WIDTH,
    ELEMENT_HEIGHT,
    ANIMATION,
];

pub const DESIGN_SIZE_MIN_PX: i64 = 40;
pub const DESIGN_SIZE_MAX_PX: i64 = 7680;

pub const MARGIN_MIN_PERCENT: i64 = 0;
pub const MARGIN_MAX_PERCENT: i64 = 45;

pub const MARGIN_SIDES: [&str; 4] = [MARGIN_TOP, MARGIN_RIGHT, MARGIN_BOTTOM, MARGIN_LEFT];

pub const TEXT_SIZE_MIN_PX: i64 = 8;
pub const TEXT_SIZE_MAX_PX: i64 = 200;

pub const CLIP_DURATION_MIN_MS: i64 = 0;
pub const CLIP_DURATION_MAX_MS: i64 = 60 * 60 * 1_000;

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

fn check_field(field: &FormField, config: &OverlayConfig) -> Result<(), OverlayError> {
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
        FormField::Duration { key, bounds, .. } => {
            bounded_int(config, key, bounds.min_ms, bounds.max_ms)
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
    let mut fields = vec![
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
    ];
    fields.extend(shared_style_fields());
    fields
}

pub(crate) fn shared_style_fields() -> Vec<SectionedField> {
    vec![
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
            FormField::Integer {
                key: TEXT_SIZE,
                label: "Text size",
                min: TEXT_SIZE_MIN_PX,
                max: TEXT_SIZE_MAX_PX,
            },
        ),
    ]
}

pub(crate) fn position_field(label: &'static str) -> SectionedField {
    in_section(
        ConfigSection::Style,
        FormField::Select {
            key: POSITION,
            label,
            options: POSITION_OPTIONS,
        },
    )
}

pub(crate) fn design_size_fields() -> [SectionedField; 2] {
    [
        in_section(
            ConfigSection::Style,
            FormField::Integer {
                key: DESIGN_WIDTH,
                label: "Source width",
                min: DESIGN_SIZE_MIN_PX,
                max: DESIGN_SIZE_MAX_PX,
            },
        ),
        in_section(
            ConfigSection::Style,
            FormField::Integer {
                key: DESIGN_HEIGHT,
                label: "Source height",
                min: DESIGN_SIZE_MIN_PX,
                max: DESIGN_SIZE_MAX_PX,
            },
        ),
    ]
}

pub(crate) fn margin_fields() -> [SectionedField; 4] {
    [
        (MARGIN_TOP, "Margin top"),
        (MARGIN_RIGHT, "Margin right"),
        (MARGIN_BOTTOM, "Margin bottom"),
        (MARGIN_LEFT, "Margin left"),
    ]
    .map(|(key, label)| {
        in_section(
            ConfigSection::Style,
            FormField::Slider {
                key,
                label,
                min: MARGIN_MIN_PERCENT,
                max: MARGIN_MAX_PERCENT,
                unit: "%",
            },
        )
    })
}

pub(crate) fn author_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: AUTHOR,
            label: "Author",
            placeholder: "PixelPal",
        },
    )
}

pub(crate) fn author_color_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: AUTHOR_COLOR,
            label: "Name color",
            placeholder: "#f9e2af",
        },
    )
}

pub(crate) fn badges_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: BADGES,
            label: "Badges",
            placeholder: "MOD",
        },
    )
}

pub(crate) fn message_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: MESSAGE,
            label: "Message",
            placeholder: "hey chat!",
        },
    )
}

pub(crate) fn chat_platform_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: PLATFORM,
            label: "Platform",
            placeholder: "twitch",
        },
    )
}

pub(crate) fn label_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: LABEL,
            label: "Label",
            placeholder: "Sub goal",
        },
    )
}

pub(crate) fn value_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: VALUE,
            label: "Current value",
            placeholder: "42",
        },
    )
}

pub(crate) fn target_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: TARGET,
            label: "Target",
            placeholder: "100",
        },
    )
}

pub(crate) fn slot_field(options: &'static [&'static str]) -> SectionedField {
    in_section(
        ConfigSection::Behavior,
        FormField::Select {
            key: SLOT,
            label: "Slot",
            options,
        },
    )
}

pub(crate) fn platform_scope_field() -> SectionedField {
    in_section(
        ConfigSection::Behavior,
        FormField::Text {
            key: PLATFORM,
            label: "Platform",
            placeholder: "All platforms",
        },
    )
}

pub(crate) fn headline_field(placeholder: &'static str) -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: HEADLINE,
            label: "Headline",
            placeholder,
        },
    )
}

pub(crate) fn subline_field(placeholder: &'static str) -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: SUBLINE,
            label: "Subline",
            placeholder,
        },
    )
}

pub(crate) fn placeholder_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: PLACEHOLDER,
            label: "Empty text",
            placeholder: "No donations yet",
        },
    )
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

pub(crate) fn icon_field() -> SectionedField {
    in_section(
        ConfigSection::Style,
        FormField::DynamicSelect {
            key: ICON,
            label: "Icon",
            options_key: ICON_OPTIONS_KEY,
        },
    )
}

pub(crate) fn sound_field() -> SectionedField {
    in_section(
        ConfigSection::Behavior,
        FormField::DynamicSelect {
            key: SOUND,
            label: "Sound",
            options_key: SOUND_OPTIONS_KEY,
        },
    )
}

pub(crate) fn speech_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::TextArea {
            key: SPEECH,
            label: "Speech",
        },
    )
}

pub(crate) fn speech_voice_field() -> SectionedField {
    in_section(
        ConfigSection::Behavior,
        FormField::DynamicSelect {
            key: SPEECH_VOICE,
            label: "Speech voice",
            options_key: ENGINE_VOICE_OPTIONS_KEY,
        },
    )
}

fn in_section(section: ConfigSection, field: FormField) -> SectionedField {
    SectionedField { section, field }
}

pub(crate) fn shared_style_defaults(
    accent: &str,
    font: &str,
    sizing: ElementSizing,
) -> OverlayConfig {
    OverlayConfig::from([
        (ACCENT.to_owned(), text(accent)),
        (FONT.to_owned(), text(font)),
        (
            TEXT_SIZE.to_owned(),
            Variant::Int(i64::from(sizing.default_text_size_px())),
        ),
    ])
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::assets::PageAssets;
    use crate::descriptor::DeliveryDisposition;
    use crate::kinds::alert::AlertOverlayKind;
    use crate::kinds::chat::ChatOverlayKind;
    use crate::motion;
    use crate::preview::{PreviewComposition, PreviewShape, compose};
    use forge_registry::{AmountUnit, DurationBounds, UnitAmountBounds};

    const PROBE_TOGGLE: &str = "probe.toggle";
    const PROBE_INTEGER: &str = "probe.integer";
    const PROBE_OPTIONAL: &str = "probe.optional";
    const PROBE_INNER: &str = "probe.inner";
    const PROBE_INNER_OPTIONS: &[&str] = &["left", "right"];
    const PROBE_AMOUNT: &str = "probe.amount";
    const PROBE_AMOUNT_UNIT: &str = "probe.amount_unit";
    const PROBE_DURATION: &str = "probe.duration";
    const PROBE_DURATION_BOUNDS: DurationBounds = DurationBounds {
        min_ms: 100,
        max_ms: 600_000,
    };
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

        fn preview(&self, config: &OverlayConfig) -> PreviewComposition {
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
}
