use forge_types::{Variant, strip_var_decoration};

use crate::config_choice::ConfigChoice;
use crate::config_problem::{ConfigReason, ValueBounds};
use crate::declared_config::{FieldLiteral, TypedField};
use crate::form::FormField;

const TOGGLE_WIRE_NAMES: [&str; 2] = ["true", "false"];

#[derive(Clone, Copy)]
pub enum TextEditor {
    Line { placeholder: &'static str },
    Area,
}

pub struct TextField {
    pub key: &'static str,
    pub label: &'static str,
    pub editor: TextEditor,
    pub default: &'static str,
    pub required: bool,
}

impl TypedField for TextField {
    type Value = String;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        match self.editor {
            TextEditor::Line { placeholder } => FormField::Text {
                key: self.key,
                label: self.label,
                placeholder,
            },
            TextEditor::Area => FormField::TextArea {
                key: self.key,
                label: self.label,
            },
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, String>, ConfigReason> {
        let Variant::String(text) = value else {
            return Err(ConfigReason::NotText);
        };
        if self.required && text.trim().is_empty() {
            return Err(ConfigReason::Required);
        }
        Ok(FieldLiteral::of_text(text).unwrap_or_else(|| FieldLiteral::Value(text.clone())))
    }

    fn parse_text(&self, text: &str) -> Result<String, ConfigReason> {
        Ok(text.to_owned())
    }
}

pub struct VariableField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub default: &'static str,
}

impl TypedField for VariableField {
    type Value = String;

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
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, String>, ConfigReason> {
        match value {
            Variant::String(text) => self.parse_text(text).map(FieldLiteral::Value),
            _ => Err(ConfigReason::NotText),
        }
    }

    fn parse_text(&self, text: &str) -> Result<String, ConfigReason> {
        let name = strip_var_decoration(text);
        if name.is_empty() {
            Err(ConfigReason::Required)
        } else {
            Ok(name)
        }
    }
}

#[derive(Clone, Copy)]
pub enum IntegerEditor {
    Number,
    Slider { unit: &'static str },
    Text { placeholder: &'static str },
}

pub struct IntegerField {
    pub key: &'static str,
    pub label: &'static str,
    pub editor: IntegerEditor,
    pub default: i64,
    pub bounds: ValueBounds,
}

impl IntegerField {
    fn bounded(&self, value: i64) -> Result<i64, ConfigReason> {
        if self.bounds.admits_whole(value) {
            Ok(value)
        } else {
            Err(ConfigReason::OutOfRange(self.bounds))
        }
    }
}

impl TypedField for IntegerField {
    type Value = i64;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        let min = self.bounds.min.unwrap_or(i64::MIN);
        let max = self.bounds.max.unwrap_or(i64::MAX);
        match self.editor {
            IntegerEditor::Number => FormField::Integer {
                key: self.key,
                label: self.label,
                min,
                max,
            },
            IntegerEditor::Slider { unit } => FormField::Slider {
                key: self.key,
                label: self.label,
                min,
                max,
                unit,
            },
            IntegerEditor::Text { placeholder } => FormField::Text {
                key: self.key,
                label: self.label,
                placeholder,
            },
        }
    }

    fn default_value(&self) -> Variant {
        match self.editor {
            IntegerEditor::Text { .. } => Variant::String(self.default.to_string()),
            IntegerEditor::Number | IntegerEditor::Slider { .. } => Variant::Int(self.default),
        }
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, i64>, ConfigReason> {
        match value {
            Variant::Int(number) => self.bounded(*number).map(FieldLiteral::Value),
            Variant::String(text) => match FieldLiteral::of_text(text) {
                Some(template) => Ok(template),
                None => self.parse_text(text).map(FieldLiteral::Value),
            },
            _ => Err(ConfigReason::NotANumber),
        }
    }

    fn parse_text(&self, text: &str) -> Result<i64, ConfigReason> {
        let typed = text.trim();
        if typed.is_empty() {
            return Err(ConfigReason::Required);
        }
        let number = typed.parse::<i64>().map_err(|_| ConfigReason::NotANumber)?;
        self.bounded(number)
    }
}

pub struct NumberField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub default: f64,
    pub bounds: ValueBounds,
}

impl NumberField {
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

impl TypedField for NumberField {
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
        Variant::String(self.default.to_string())
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

pub struct ToggleField {
    pub key: &'static str,
    pub label: &'static str,
    pub default: bool,
}

impl TypedField for ToggleField {
    type Value = bool;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::Toggle {
            key: self.key,
            label: self.label,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::Bool(self.default)
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, bool>, ConfigReason> {
        match value {
            Variant::Bool(flag) => Ok(FieldLiteral::Value(*flag)),
            Variant::String(text) => match FieldLiteral::of_text(text) {
                Some(template) => Ok(template),
                None => self.parse_text(text).map(FieldLiteral::Value),
            },
            _ => Err(ConfigReason::choices(TOGGLE_WIRE_NAMES)),
        }
    }

    fn parse_text(&self, text: &str) -> Result<bool, ConfigReason> {
        text.trim()
            .parse::<bool>()
            .map_err(|_| ConfigReason::choices(TOGGLE_WIRE_NAMES))
    }
}

pub struct ChoiceField<T: ConfigChoice> {
    pub key: &'static str,
    pub label: &'static str,
    pub default: T,
}

impl<T: ConfigChoice> TypedField for ChoiceField<T> {
    type Value = T;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::Select {
            key: self.key,
            label: self.label,
            options: T::WIRE_NAMES,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.wire_name().to_owned())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, T>, ConfigReason> {
        match value {
            Variant::String(text) => self.parse_text(text).map(FieldLiteral::Value),
            _ => Err(ConfigReason::choices(T::WIRE_NAMES.iter().copied())),
        }
    }

    fn parse_text(&self, text: &str) -> Result<T, ConfigReason> {
        T::parse(text).ok_or_else(|| ConfigReason::choices(T::WIRE_NAMES.iter().copied()))
    }
}
