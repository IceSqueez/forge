use forge_types::{ArgStack, SubActionConfig, Variant};

use crate::config_problem::{ConfigProblem, ConfigReason};
use crate::declared_config::{FieldLiteral, Interpolation, TypedField, peek_stored, read_stored};
use crate::declared_fields::{TextEditor, TextField, ToggleField};
use crate::form::FormField;

#[derive(Clone, Copy)]
pub enum TextPicker {
    Options { options_key: &'static str },
    File,
}

pub struct PickedTextField {
    pub key: &'static str,
    pub label: &'static str,
    pub picker: TextPicker,
    pub default: &'static str,
    pub required: bool,
}

impl PickedTextField {
    const fn as_text(&self) -> TextField {
        TextField {
            key: self.key,
            label: self.label,
            editor: TextEditor::Area,
            default: self.default,
            required: self.required,
        }
    }
}

impl TypedField for PickedTextField {
    type Value = String;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        match self.picker {
            TextPicker::Options { options_key } => FormField::DynamicSelect {
                key: self.key,
                label: self.label,
                options_key,
            },
            TextPicker::File => FormField::FilePicker {
                key: self.key,
                label: self.label,
            },
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, String>, ConfigReason> {
        self.as_text().literal(value)
    }

    fn parse_text(&self, text: &str) -> Result<String, ConfigReason> {
        self.as_text().parse_text(text)
    }
}

#[derive(Clone, Copy)]
pub enum OptionalSwitch {
    SharedKey {
        label: Option<&'static str>,
    },
    Toggle {
        key: &'static str,
        label: &'static str,
        default_on: bool,
    },
}

pub struct OptionalField<F> {
    pub field: F,
    pub switch: OptionalSwitch,
}

impl<F> OptionalField<F> {
    pub const fn shared(field: F) -> Self {
        Self {
            field,
            switch: OptionalSwitch::SharedKey { label: None },
        }
    }
}

impl<F: TypedField> OptionalField<F> {
    fn toggle(key: &'static str, label: &'static str, default_on: bool) -> ToggleField {
        ToggleField {
            key,
            label,
            default: default_on,
        }
    }
}

impl<F: TypedField> TypedField for OptionalField<F> {
    type Value = Option<F::Value>;

    fn key(&self) -> &'static str {
        match self.switch {
            OptionalSwitch::SharedKey { .. } => self.field.key(),
            OptionalSwitch::Toggle { key, .. } => key,
        }
    }

    fn form_field(&self) -> FormField {
        let inner = self.field.form_field();
        let label = match self.switch {
            OptionalSwitch::SharedKey { label } => label.unwrap_or_else(|| inner.label()),
            OptionalSwitch::Toggle { label, .. } => label,
        };
        FormField::Optional {
            key: self.key(),
            label,
            inner: Box::new(inner),
        }
    }

    fn default_value(&self) -> Variant {
        match self.switch {
            OptionalSwitch::SharedKey { .. } => Variant::String(String::new()),
            OptionalSwitch::Toggle { default_on, .. } => Variant::Bool(default_on),
        }
    }

    fn write_default(&self, config: &mut SubActionConfig) {
        if let OptionalSwitch::Toggle {
            key, default_on, ..
        } = self.switch
        {
            config.insert(key.to_owned(), Variant::Bool(default_on));
            self.field.write_default(config);
        }
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Self::Value>, ConfigReason> {
        match value {
            Variant::String(text) if text.trim().is_empty() => {
                return Ok(FieldLiteral::Value(None));
            }
            Variant::Bool(_) if self.field.literal(value).is_err() => {
                return Ok(FieldLiteral::Value(None));
            }
            _ => {}
        }
        Ok(match self.field.literal(value)? {
            FieldLiteral::Value(parsed) => FieldLiteral::Value(Some(parsed)),
            FieldLiteral::Template(text) => FieldLiteral::Template(text),
        })
    }

    fn parse_text(&self, text: &str) -> Result<Self::Value, ConfigReason> {
        if text.trim().is_empty() {
            Ok(None)
        } else {
            self.field.parse_text(text).map(Some)
        }
    }

    fn read(
        &self,
        config: &SubActionConfig,
        args: &ArgStack,
    ) -> Result<Self::Value, ConfigProblem> {
        match self.switch {
            OptionalSwitch::SharedKey { .. } => read_stored(self, config, args),
            OptionalSwitch::Toggle {
                key,
                label,
                default_on,
            } => match Self::toggle(key, label, default_on).read(config, args)? {
                true => self.field.read(config, args).map(Some),
                false => Ok(None),
            },
        }
    }

    fn peek(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<Option<Self::Value>, ConfigProblem> {
        match self.switch {
            OptionalSwitch::SharedKey { .. } => peek_stored(self, config, interpolation),
            OptionalSwitch::Toggle {
                key,
                label,
                default_on,
            } => match Self::toggle(key, label, default_on).peek(config, interpolation)? {
                Some(true) => Ok(self.field.peek(config, interpolation)?.map(Some)),
                Some(false) => Ok(Some(None)),
                None => Ok(None),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_types::{ArgStack, SubActionConfig};

    use super::*;
    use crate::config_problem::ConfigProblem;
    use crate::config_problem::ValueBounds;
    use crate::declared_config::{ConfigSpec, Interpolation};
    use crate::declared_fields::{NumberField, StoredNumber};

    const SCENE: PickedTextField = PickedTextField {
        key: "scene",
        label: "Scene",
        picker: TextPicker::Options {
            options_key: "scene_names",
        },
        default: "",
        required: true,
    };
    const SNAPSHOT: PickedTextField = PickedTextField {
        key: "snapshot",
        label: "Save To",
        picker: TextPicker::File,
        default: "shot.png",
        required: false,
    };
    const HOLD: NumberField = NumberField {
        key: "hold",
        label: "Hold",
        placeholder: "0",
        default: StoredNumber::Float(0.0),
        bounds: ValueBounds {
            min: Some(0),
            max: Some(600),
        },
    };
    const GAIN: NumberField = NumberField {
        key: "gain",
        label: "Gain",
        placeholder: "0.0",
        default: StoredNumber::Text("0.0"),
        bounds: ValueBounds::UNBOUNDED,
    };
    const MAYBE_HOLD: OptionalField<NumberField> = OptionalField::shared(HOLD);
    const MAYBE_NOTE: OptionalField<TextField> = OptionalField::shared(TextField {
        key: "note",
        label: "Note",
        editor: TextEditor::Line { placeholder: "" },
        default: "",
        required: true,
    });
    const MAYBE_FLAG: OptionalField<ToggleField> = OptionalField::shared(ToggleField {
        key: "flag",
        label: "Flag",
        default: false,
    });
    const REQUIRED_NOTE: TextField = TextField {
        key: "required_note",
        label: "Note",
        editor: TextEditor::Line { placeholder: "" },
        default: "",
        required: true,
    };
    const SPEC: ConfigSpec = ConfigSpec::new(&[&SCENE, &HOLD, &GAIN, &MAYBE_NOTE]);

    fn args() -> ArgStack {
        ArgStack::new()
            .set("who".to_owned(), Variant::String("ann".to_owned()))
            .set("n".to_owned(), Variant::Int(7))
            .set("big".to_owned(), Variant::Int(700))
            .set("nothing".to_owned(), Variant::String(String::new()))
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn read<F: TypedField>(field: &F, value: Variant) -> Result<F::Value, ConfigProblem> {
        field.read(
            &SubActionConfig::from([(field.key().to_owned(), value)]),
            &args(),
        )
    }

    fn rejected<T>(field: &impl TypedField, reason: ConfigReason) -> Result<T, ConfigProblem> {
        Err(ConfigProblem::field(field.key(), reason))
    }

    #[test]
    fn picked_text_field_offers_its_picker_in_the_form() {
        assert!(matches!(
            SCENE.form_field(),
            FormField::DynamicSelect {
                key: "scene",
                label: "Scene",
                options_key: "scene_names",
            }
        ));
        assert!(matches!(
            SNAPSHOT.form_field(),
            FormField::FilePicker {
                key: "snapshot",
                label: "Save To",
            }
        ));
    }

    #[test]
    fn picked_text_field_reads_like_a_text_field_of_the_same_requirement() {
        for (field, value, expected) in [
            (&SCENE, text("%who% cam"), Ok("ann cam".to_owned())),
            (&SCENE, text(" "), rejected(&SCENE, ConfigReason::Required)),
            (
                &SCENE,
                Variant::Int(1),
                rejected(&SCENE, ConfigReason::NotText),
            ),
            (&SNAPSHOT, text(""), Ok(String::new())),
        ] {
            assert_eq!(read(field, value.clone()), expected, "{value:?}");
        }
    }

    #[test]
    fn number_field_stores_its_default_as_a_float_or_as_text() {
        assert_eq!(
            (HOLD.default_value(), GAIN.default_value()),
            (Variant::Float(0.0), text("0.0")),
        );
    }

    #[test]
    fn number_field_reads_a_float_or_text_default_when_the_value_is_absent() {
        let args = args();
        assert_eq!(
            (
                HOLD.read(&SubActionConfig::new(), &args),
                GAIN.read(&SubActionConfig::new(), &args),
            ),
            (Ok(0.0), Ok(0.0)),
        );
    }

    #[test]
    fn number_field_reads_a_template_resolving_to_blank_as_required() {
        assert_eq!(
            read(&HOLD, text("%nothing%")),
            rejected(&HOLD, ConfigReason::Required)
        );
    }

    #[test]
    fn number_field_defers_a_template_until_run_time() {
        assert_eq!(
            HOLD.peek(
                &SubActionConfig::from([("hold".to_owned(), text("%big%"))]),
                Interpolation::Deferred,
            ),
            Ok(None),
        );
    }

    #[test]
    fn optional_field_reads_absent_blank_or_blank_after_interpolation_as_none() {
        let args = args();
        assert_eq!(MAYBE_HOLD.read(&SubActionConfig::new(), &args), Ok(None));
        for value in [text(""), text("  "), text("%nothing%")] {
            assert_eq!(read(&MAYBE_HOLD, value.clone()), Ok(None), "{value:?}");
            assert_eq!(read(&MAYBE_NOTE, value.clone()), Ok(None), "{value:?}");
        }
    }

    #[test]
    fn optional_field_reads_a_present_value_through_its_inner_field() {
        for (value, expected) in [
            (text("2.5"), Ok(Some(2.5))),
            (Variant::Int(3), Ok(Some(3.0))),
            (text("%n%"), Ok(Some(7.0))),
            (
                text("601"),
                rejected(&HOLD, ConfigReason::OutOfRange(HOLD.bounds)),
            ),
            (
                text("%big%"),
                rejected(&HOLD, ConfigReason::OutOfRange(HOLD.bounds)),
            ),
            (Variant::Bool(false), Ok(None)),
            (Variant::Bool(true), Ok(None)),
        ] {
            assert_eq!(read(&MAYBE_HOLD, value.clone()), expected, "{value:?}");
        }
    }

    #[test]
    fn optional_field_passes_a_bool_through_when_the_inner_field_takes_a_bool() {
        assert_eq!(read(&MAYBE_FLAG, Variant::Bool(true)), Ok(Some(true)));
        assert_eq!(read(&MAYBE_FLAG, Variant::Bool(false)), Ok(Some(false)));
    }

    #[test]
    fn required_text_field_rejects_a_template_resolving_to_whitespace() {
        let args = args().set("blank".to_owned(), text("   "));
        for value in [text("%blank%"), text("%nothing%"), text("  ")] {
            let config = SubActionConfig::from([("required_note".to_owned(), value.clone())]);
            assert_eq!(
                REQUIRED_NOTE.read(&config, &args),
                rejected(&REQUIRED_NOTE, ConfigReason::Required),
                "{value:?}",
            );
        }
    }

    #[test]
    fn optional_field_is_left_out_of_the_default_config() {
        assert_eq!(
            SPEC.default_config().into_keys().collect::<Vec<_>>(),
            vec!["gain".to_owned(), "hold".to_owned(), "scene".to_owned()],
        );
    }

    #[test]
    fn optional_field_wraps_its_inner_form_field_under_the_inner_label() {
        let FormField::Optional { key, label, inner } = MAYBE_HOLD.form_field() else {
            panic!("expected an optional form field");
        };
        assert_eq!((key, label), ("hold", "Hold"));
        assert!(matches!(*inner, FormField::Text { key: "hold", .. }));
    }
}
