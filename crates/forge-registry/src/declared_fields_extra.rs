use forge_types::Variant;

use crate::config_problem::{ConfigReason, ValueBounds};
use crate::declared_config::{FieldLiteral, TypedField};
use crate::declared_fields::{TextEditor, TextField};
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
pub enum StoredNumber {
    Float(f64),
    Text(&'static str),
}

pub struct DecimalField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub default: StoredNumber,
    pub bounds: ValueBounds,
}

impl DecimalField {
    fn bounded(&self, value: f64) -> Result<f64, ConfigReason> {
        if !value.is_finite() {
            Err(ConfigReason::NotANumber)
        } else if self.bounds.admits(value) {
            Ok(value)
        } else {
            Err(ConfigReason::OutOfRange(self.bounds))
        }
    }
}

impl TypedField for DecimalField {
    type Value = f64;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::Text {
            key: self.key,
            label: self.label,
            placeholder: self.placeholder,
        }
    }

    fn default_value(&self) -> Variant {
        match self.default {
            StoredNumber::Float(number) => Variant::Float(number),
            StoredNumber::Text(text) => Variant::String(text.to_owned()),
        }
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, f64>, ConfigReason> {
        match value {
            Variant::Float(number) => self.bounded(*number).map(FieldLiteral::Value),
            Variant::Int(number) => self.bounded(*number as f64).map(FieldLiteral::Value),
            Variant::String(text) => match FieldLiteral::of_text(text) {
                Some(template) => Ok(template),
                None => self.parse_text(text).map(FieldLiteral::Value),
            },
            _ => Err(ConfigReason::NotANumber),
        }
    }

    fn parse_text(&self, text: &str) -> Result<f64, ConfigReason> {
        let typed = text.trim();
        if typed.is_empty() {
            return Err(ConfigReason::Required);
        }
        let number = typed.parse::<f64>().map_err(|_| ConfigReason::NotANumber)?;
        self.bounded(number)
    }
}

pub struct OptionalField<F> {
    pub field: F,
}

impl<F: TypedField> TypedField for OptionalField<F> {
    type Value = Option<F::Value>;

    fn key(&self) -> &'static str {
        self.field.key()
    }

    fn form_field(&self) -> FormField {
        let inner = self.field.form_field();
        FormField::Optional {
            key: self.field.key(),
            label: inner.label(),
            inner: Box::new(inner),
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(String::new())
    }

    fn seeds_default(&self) -> bool {
        false
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Self::Value>, ConfigReason> {
        if let Variant::String(text) = value
            && text.trim().is_empty()
        {
            return Ok(FieldLiteral::Value(None));
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
}
