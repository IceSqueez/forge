use std::marker::PhantomData;
use std::str::FromStr;
use std::time::Duration;

use serde_json::{Map as JsonMap, Value as JsonValue};

use forge_types::{SubActionConfig, Variant, datetime_from_text, strip_var_decoration};
use time::OffsetDateTime;

use crate::config_choice::ConfigChoice;
use crate::config_messages::{
    not_a_datetime, not_a_json_object, not_a_step_list, not_json, timestamp_out_of_range,
    unknown_entry,
};
use crate::config_problem::{ConfigProblem, ConfigReason, ValueBounds};
use crate::declared_config::{FieldLiteral, Interpolation, TypedField};
use crate::duration_bounds::DurationBounds;
use crate::form::{CodeKind, CodeLanguage, FormField};

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
        if self.required && text.trim().is_empty() {
            Err(ConfigReason::Required)
        } else {
            Ok(text.to_owned())
        }
    }
}

pub struct TemplateField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub default: &'static str,
    pub required: bool,
}

impl TypedField for TemplateField {
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
        if self.required && text.trim().is_empty() {
            Err(ConfigReason::Required)
        } else {
            Ok(text.to_owned())
        }
    }
}

pub struct ValueField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub default: &'static str,
}

impl TypedField for ValueField {
    type Value = Variant;

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

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, Variant>, ConfigReason> {
        match value {
            Variant::String(text) => {
                Ok(FieldLiteral::of_text(text)
                    .unwrap_or_else(|| FieldLiteral::Value(value.clone())))
            }
            other => Ok(FieldLiteral::Value(other.clone())),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Variant, ConfigReason> {
        Ok(Variant::String(text.to_owned()))
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

#[derive(Clone, Copy)]
pub enum StoredNumber {
    Float(f64),
    Text(&'static str),
}

pub struct NumberField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub default: StoredNumber,
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

pub struct ListedChoiceField {
    pub key: &'static str,
    pub label: &'static str,
    pub default: &'static str,
    pub options: fn() -> &'static [&'static str],
}

impl ListedChoiceField {
    fn invalid_choice(&self) -> ConfigReason {
        ConfigReason::choices((self.options)().iter().copied())
    }
}

impl TypedField for ListedChoiceField {
    type Value = &'static str;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::Select {
            key: self.key,
            label: self.label,
            options: (self.options)(),
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, &'static str>, ConfigReason> {
        match value {
            Variant::String(text) => self.parse_text(text).map(FieldLiteral::Value),
            _ => Err(self.invalid_choice()),
        }
    }

    fn parse_text(&self, text: &str) -> Result<&'static str, ConfigReason> {
        (self.options)()
            .iter()
            .copied()
            .find(|option| *option == text)
            .ok_or_else(|| self.invalid_choice())
    }
}
pub struct CodeField {
    pub key: &'static str,
    pub label: &'static str,
    pub language: CodeLanguage,
    pub kind: CodeKind,
    pub default: &'static str,
    pub required: bool,
}

impl TypedField for CodeField {
    type Value = String;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::Code {
            key: self.key,
            label: self.label,
            language: self.language,
            kind: self.kind,
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
        if self.required && text.trim().is_empty() {
            Err(ConfigReason::Required)
        } else {
            Ok(text.to_owned())
        }
    }
}

pub struct DynamicSelectField<T> {
    pub key: &'static str,
    pub label: &'static str,
    pub options_key: &'static str,
    pub default: &'static str,
    value: PhantomData<fn() -> T>,
}

impl<T> DynamicSelectField<T> {
    pub const fn new(key: &'static str, label: &'static str, options_key: &'static str) -> Self {
        Self {
            key,
            label,
            options_key,
            default: "",
            value: PhantomData,
        }
    }

    pub const fn with_default(self, default: &'static str) -> Self {
        Self { default, ..self }
    }
}

impl<T: FromStr> TypedField for DynamicSelectField<T> {
    type Value = T;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::DynamicSelect {
            key: self.key,
            label: self.label,
            options_key: self.options_key,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, T>, ConfigReason> {
        selected_text(value, |text| self.parse_text(text))
    }

    fn parse_text(&self, text: &str) -> Result<T, ConfigReason> {
        parse_selected(text)
    }
}

pub struct BlankableSelectField<T> {
    pub key: &'static str,
    pub label: &'static str,
    pub options_key: &'static str,
    value: PhantomData<fn() -> T>,
}

impl<T> BlankableSelectField<T> {
    pub const fn new(key: &'static str, label: &'static str, options_key: &'static str) -> Self {
        Self {
            key,
            label,
            options_key,
            value: PhantomData,
        }
    }
}

impl<T: FromStr> TypedField for BlankableSelectField<T> {
    type Value = Option<T>;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::DynamicSelect {
            key: self.key,
            label: self.label,
            options_key: self.options_key,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(String::new())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, Option<T>>, ConfigReason> {
        selected_text(value, |text| self.parse_text(text))
    }

    fn parse_text(&self, text: &str) -> Result<Option<T>, ConfigReason> {
        if text.trim().is_empty() {
            Ok(None)
        } else {
            parse_selected(text).map(Some)
        }
    }
}

pub struct DependentSelectField {
    pub key: &'static str,
    pub label: &'static str,
    pub options_prefix: &'static str,
    pub depends_on: &'static str,
}

impl TypedField for DependentSelectField {
    type Value = String;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::DependentSelect {
            key: self.key,
            label: self.label,
            options_prefix: self.options_prefix,
            depends_on: self.depends_on,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(String::new())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, String>, ConfigReason> {
        selected_text(value, |text| self.parse_text(text))
    }

    fn parse_text(&self, text: &str) -> Result<String, ConfigReason> {
        parse_selected(text)
    }
}

fn selected_text<'a, T>(
    value: &'a Variant,
    parse: impl FnOnce(&str) -> Result<T, ConfigReason>,
) -> Result<FieldLiteral<'a, T>, ConfigReason> {
    let Variant::String(text) = value else {
        return Err(ConfigReason::NotText);
    };
    match FieldLiteral::of_text(text) {
        Some(template) => Ok(template),
        None => parse(text).map(FieldLiteral::Value),
    }
}

fn parse_selected<T: FromStr>(text: &str) -> Result<T, ConfigReason> {
    let selected = text.trim();
    if selected.is_empty() {
        return Err(ConfigReason::Required);
    }
    selected.parse::<T>().map_err(|_| unknown_entry())
}

pub struct DurationField {
    pub key: &'static str,
    pub label: &'static str,
    pub default: i64,
    pub bounds: DurationBounds,
}

impl DurationField {
    fn bounded(&self, stored: i64) -> Result<Duration, ConfigReason> {
        let allowed = ValueBounds::between(self.bounds.min, self.bounds.max);
        match u64::try_from(stored) {
            Ok(amount) if allowed.admits_whole(stored) => Ok(self.bounds.duration_of(amount)),
            _ => Err(ConfigReason::OutOfRange(allowed)),
        }
    }
}

impl TypedField for DurationField {
    type Value = Duration;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::Duration {
            key: self.key,
            label: self.label,
            bounds: self.bounds,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::Int(self.default)
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, Duration>, ConfigReason> {
        match value {
            Variant::Int(stored) => self.bounded(*stored).map(FieldLiteral::Value),
            Variant::String(text) => match FieldLiteral::of_text(text) {
                Some(template) => Ok(template),
                None => self.parse_text(text).map(FieldLiteral::Value),
            },
            _ => Err(ConfigReason::NotANumber),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Duration, ConfigReason> {
        let typed = text.trim();
        if typed.is_empty() {
            return Err(ConfigReason::Required);
        }
        let stored = typed.parse::<i64>().map_err(|_| ConfigReason::NotANumber)?;
        self.bounded(stored)
    }
}

pub struct DateTimeField {
    pub key: &'static str,
    pub label: &'static str,
    pub default: &'static str,
}

impl TypedField for DateTimeField {
    type Value = OffsetDateTime;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::DateTime {
            key: self.key,
            label: self.label,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, OffsetDateTime>, ConfigReason> {
        match value {
            Variant::Datetime(instant) => Ok(FieldLiteral::Value(*instant)),
            Variant::Int(seconds) => unix_instant(*seconds).map(FieldLiteral::Value),
            Variant::String(text) => match FieldLiteral::of_text(text) {
                Some(template) => Ok(template),
                None => self.parse_text(text).map(FieldLiteral::Value),
            },
            _ => Err(ConfigReason::NotText),
        }
    }

    fn parse_text(&self, text: &str) -> Result<OffsetDateTime, ConfigReason> {
        let typed = text.trim();
        if typed.is_empty() {
            return Err(ConfigReason::Required);
        }
        if let Some(instant) = datetime_from_text(typed) {
            return Ok(instant);
        }
        match typed.parse::<i64>() {
            Ok(seconds) => unix_instant(seconds),
            Err(_) => Err(not_a_datetime()),
        }
    }
}

fn unix_instant(seconds: i64) -> Result<OffsetDateTime, ConfigReason> {
    OffsetDateTime::from_unix_timestamp(seconds).map_err(|_| timestamp_out_of_range())
}

#[derive(Clone, Copy)]
pub enum ChainLayout {
    Steps,
    Cases,
}

pub struct StepChainField {
    pub key: &'static str,
    pub label: &'static str,
    pub layout: ChainLayout,
    pub seeds_empty_list: bool,
}

impl crate::declared_config::DeclaredField for StepChainField {
    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        match self.layout {
            ChainLayout::Steps => FormField::SubChain {
                key: self.key,
                label: self.label,
            },
            ChainLayout::Cases => FormField::CaseList {
                key: self.key,
                label: self.label,
            },
        }
    }

    fn write_default(&self, config: &mut SubActionConfig) {
        if self.seeds_empty_list {
            config.insert(self.key.to_owned(), Variant::Array(Vec::new()));
        }
    }

    fn check(
        &self,
        config: &SubActionConfig,
        _interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        match config.get(self.key) {
            None | Some(Variant::Array(_)) => Ok(()),
            Some(_) => Err(ConfigProblem::field(self.key, not_a_step_list())),
        }
    }
}

pub struct JsonObjectField {
    pub key: &'static str,
    pub label: &'static str,
    pub default: &'static str,
}

impl TypedField for JsonObjectField {
    type Value = JsonMap<String, JsonValue>;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::TextArea {
            key: self.key,
            label: self.label,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Self::Value>, ConfigReason> {
        match value {
            Variant::Object(_) => match value.to_plain_json() {
                JsonValue::Object(map) => Ok(FieldLiteral::Value(map)),
                _ => Err(not_a_json_object()),
            },
            Variant::String(text) => self.parse_text(text).map(FieldLiteral::Value),
            _ => Err(ConfigReason::NotText),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Self::Value, ConfigReason> {
        if text.trim().is_empty() {
            return Ok(JsonMap::new());
        }
        match serde_json::from_str::<JsonValue>(text) {
            Ok(JsonValue::Object(map)) => Ok(map),
            Ok(_) => Err(not_a_json_object()),
            Err(error) => Err(not_json(error)),
        }
    }
}

pub struct LinesField {
    pub key: &'static str,
    pub label: &'static str,
}

impl TypedField for LinesField {
    type Value = Vec<Variant>;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::TextArea {
            key: self.key,
            label: self.label,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(String::new())
    }

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Vec<Variant>>, ConfigReason> {
        match value {
            Variant::Array(entries) => Ok(FieldLiteral::Value(entries.clone())),
            Variant::String(text) => self.parse_text(text).map(FieldLiteral::Value),
            _ => Err(ConfigReason::NotText),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Vec<Variant>, ConfigReason> {
        Ok(text
            .lines()
            .map(|line| Variant::String(line.to_owned()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use forge_types::{ArgStack, SubActionConfig};

    use super::*;
    use crate::config_problem::ConfigProblem;
    use crate::declared_config::Interpolation;

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Pace {
        Slow,
        Fast,
    }

    impl ConfigChoice for Pace {
        const ALL: &'static [Self] = &[Pace::Slow, Pace::Fast];
        const WIRE_NAMES: &'static [&'static str] = &["slow", "fast"];

        fn wire_name(self) -> &'static str {
            match self {
                Pace::Slow => "slow",
                Pace::Fast => "fast",
            }
        }
    }

    const MESSAGE: TextField = TextField {
        key: "message",
        label: "Message",
        editor: TextEditor::Line { placeholder: "" },
        default: "hello",
        required: true,
    };
    const NOTE: TextField = TextField {
        key: "note",
        label: "Note",
        editor: TextEditor::Area,
        default: "",
        required: false,
    };
    const TARGET: VariableField = VariableField {
        key: "target",
        label: "Target",
        placeholder: "",
        default: "result",
    };
    const COUNT: IntegerField = IntegerField {
        key: "count",
        label: "Count",
        editor: IntegerEditor::Number,
        default: 5,
        bounds: ValueBounds {
            min: Some(1),
            max: Some(10),
        },
    };
    const TYPED_COUNT: IntegerField = IntegerField {
        key: "typed_count",
        label: "Count",
        editor: IntegerEditor::Text { placeholder: "" },
        default: 5,
        bounds: ValueBounds::UNBOUNDED,
    };
    const RATIO: NumberField = NumberField {
        key: "ratio",
        label: "Ratio",
        placeholder: "",
        default: StoredNumber::Text("0.5"),
        bounds: ValueBounds {
            min: Some(0),
            max: Some(10),
        },
    };
    const ENABLED: ToggleField = ToggleField {
        key: "enabled",
        label: "Enabled",
        default: true,
    };
    const PACE: ChoiceField<Pace> = ChoiceField {
        key: "pace",
        label: "Pace",
        default: Pace::Slow,
    };

    fn args() -> ArgStack {
        ArgStack::new()
            .set("who".to_owned(), Variant::String("ann".to_owned()))
            .set("n".to_owned(), Variant::Int(7))
            .set("big".to_owned(), Variant::Int(50))
            .set("flag".to_owned(), Variant::Bool(false))
    }

    fn stored(key: &str, value: Variant) -> SubActionConfig {
        SubActionConfig::from([(key.to_owned(), value)])
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn read<F: TypedField>(field: &F, value: Variant) -> Result<F::Value, ConfigProblem> {
        field.read(&stored(field.key(), value), &args())
    }

    fn rejected<T>(field: &impl TypedField, reason: ConfigReason) -> Result<T, ConfigProblem> {
        Err(ConfigProblem::field(field.key(), reason))
    }

    #[test]
    fn every_field_reads_its_own_default_from_an_empty_config() {
        let empty = SubActionConfig::new();
        let args = args();

        assert_eq!(MESSAGE.read(&empty, &args), Ok("hello".to_owned()));
        assert_eq!(NOTE.read(&empty, &args), Ok(String::new()));
        assert_eq!(TARGET.read(&empty, &args), Ok("result".to_owned()));
        assert_eq!(COUNT.read(&empty, &args), Ok(5));
        assert_eq!(TYPED_COUNT.read(&empty, &args), Ok(5));
        assert_eq!(RATIO.read(&empty, &args), Ok(0.5));
        assert_eq!(ENABLED.read(&empty, &args), Ok(true));
        assert_eq!(PACE.read(&empty, &args), Ok(Pace::Slow));
    }

    #[test]
    fn integer_and_number_defaults_are_stored_as_the_editor_holds_them() {
        assert_eq!(
            (
                COUNT.default_value(),
                TYPED_COUNT.default_value(),
                RATIO.default_value(),
                PACE.default_value(),
            ),
            (Variant::Int(5), text("5"), text("0.5"), text("slow")),
        );
    }

    #[test]
    fn form_fields_follow_the_editor_and_open_unbounded_integer_sides_to_the_extremes() {
        assert!(matches!(
            MESSAGE.form_field(),
            FormField::Text { key: "message", .. }
        ));
        assert!(matches!(
            NOTE.form_field(),
            FormField::TextArea { key: "note", .. }
        ));
        assert!(matches!(
            COUNT.form_field(),
            FormField::Integer {
                key: "count",
                min: 1,
                max: 10,
                ..
            }
        ));
        assert!(matches!(
            IntegerField {
                editor: IntegerEditor::Slider { unit: "s" },
                bounds: ValueBounds::at_least(0),
                ..COUNT
            }
            .form_field(),
            FormField::Slider {
                min: 0,
                max: i64::MAX,
                unit: "s",
                ..
            }
        ));
        assert!(matches!(
            IntegerField {
                editor: IntegerEditor::Number,
                ..TYPED_COUNT
            }
            .form_field(),
            FormField::Integer {
                min: i64::MIN,
                max: i64::MAX,
                ..
            }
        ));
        assert!(matches!(
            TYPED_COUNT.form_field(),
            FormField::Text {
                key: "typed_count",
                ..
            }
        ));
        assert!(matches!(
            PACE.form_field(),
            FormField::Select {
                key: "pace",
                options: ["slow", "fast"],
                ..
            }
        ));
    }

    #[test]
    fn text_field_reads_literals_interpolates_templates_and_refuses_blank_or_non_text() {
        assert_eq!(read(&MESSAGE, text("hi")), Ok("hi".to_owned()));
        assert_eq!(
            read(&MESSAGE, text("%who% here")),
            Ok("ann here".to_owned())
        );
        assert_eq!(read(&MESSAGE, text("100%")), Ok("100%".to_owned()));
        assert_eq!(
            read(&MESSAGE, text("")),
            rejected(&MESSAGE, ConfigReason::Required)
        );
        assert_eq!(
            read(&MESSAGE, text(" \t ")),
            rejected(&MESSAGE, ConfigReason::Required)
        );
        assert_eq!(
            read(&MESSAGE, Variant::Int(3)),
            rejected(&MESSAGE, ConfigReason::NotText)
        );
    }

    #[test]
    fn optional_text_field_accepts_blank_text() {
        assert_eq!(read(&NOTE, text("  ")), Ok("  ".to_owned()));
    }

    #[test]
    fn variable_field_peels_decoration_without_interpolating_and_refuses_empty_names() {
        for (value, expected) in [
            (text("score"), Ok("score".to_owned())),
            (text(" %score% "), Ok("score".to_owned())),
            (text("%who%"), Ok("who".to_owned())),
            (text(""), rejected(&TARGET, ConfigReason::Required)),
            (text("%%"), rejected(&TARGET, ConfigReason::Required)),
            (text("% %"), rejected(&TARGET, ConfigReason::Required)),
            (
                Variant::Bool(true),
                rejected(&TARGET, ConfigReason::NotText),
            ),
        ] {
            assert_eq!(read(&TARGET, value.clone()), expected, "{value:?}");
        }
    }

    #[test]
    fn integer_field_admits_its_bounds_and_refuses_one_past_each() {
        let out_of_range = || rejected(&COUNT, ConfigReason::OutOfRange(COUNT.bounds));
        for (value, expected) in [
            (Variant::Int(0), out_of_range()),
            (Variant::Int(1), Ok(1)),
            (Variant::Int(10), Ok(10)),
            (Variant::Int(11), out_of_range()),
            (text("0"), out_of_range()),
            (text(" 10 "), Ok(10)),
            (text("11"), out_of_range()),
        ] {
            assert_eq!(read(&COUNT, value.clone()), expected, "{value:?}");
        }
    }

    #[test]
    fn integer_field_refuses_blank_unparseable_and_non_numeric_values() {
        for (value, reason) in [
            (text(""), ConfigReason::Required),
            (text("  "), ConfigReason::Required),
            (text("five"), ConfigReason::NotANumber),
            (text("2.5"), ConfigReason::NotANumber),
            (text("99999999999999999999"), ConfigReason::NotANumber),
            (Variant::Bool(true), ConfigReason::NotANumber),
            (Variant::float(3.0).unwrap(), ConfigReason::NotANumber),
        ] {
            assert_eq!(
                read(&COUNT, value.clone()),
                rejected(&COUNT, reason),
                "{value:?}"
            );
        }
    }

    #[test]
    fn integer_field_checks_an_interpolated_template_like_a_typed_value() {
        for (template, expected) in [
            ("%n%", Ok(7)),
            (
                "%big%",
                rejected(&COUNT, ConfigReason::OutOfRange(COUNT.bounds)),
            ),
            ("%who%", rejected(&COUNT, ConfigReason::NotANumber)),
            ("%missing%", rejected(&COUNT, ConfigReason::NotANumber)),
        ] {
            assert_eq!(read(&COUNT, text(template)), expected, "{template}");
        }
    }

    #[test]
    fn number_field_admits_its_bounds_and_refuses_values_past_them() {
        let out_of_range = || rejected(&RATIO, ConfigReason::OutOfRange(RATIO.bounds));
        for (value, expected) in [
            (Variant::float(-0.001).unwrap(), out_of_range()),
            (Variant::float(0.0).unwrap(), Ok(0.0)),
            (Variant::Int(10), Ok(10.0)),
            (Variant::float(10.001).unwrap(), out_of_range()),
            (Variant::Int(11), out_of_range()),
            (text(" 2.5 "), Ok(2.5)),
            (text("%n%"), Ok(7.0)),
            (text("%big%"), out_of_range()),
        ] {
            assert_eq!(read(&RATIO, value.clone()), expected, "{value:?}");
        }
    }

    #[test]
    fn number_field_refuses_blank_non_finite_and_non_numeric_values() {
        for (value, reason) in [
            (text(""), ConfigReason::Required),
            (text("NaN"), ConfigReason::NotANumber),
            (text("inf"), ConfigReason::NotANumber),
            (text("half"), ConfigReason::NotANumber),
            (Variant::Float(f64::NAN), ConfigReason::NotANumber),
            (Variant::Float(f64::INFINITY), ConfigReason::NotANumber),
            (Variant::Bool(false), ConfigReason::NotANumber),
        ] {
            assert_eq!(
                read(&RATIO, value.clone()),
                rejected(&RATIO, reason),
                "{value:?}"
            );
        }
    }

    #[test]
    fn toggle_field_reads_flags_wire_text_and_templates_and_names_its_choices_otherwise() {
        let choices = || rejected(&ENABLED, ConfigReason::choices(["true", "false"]));
        for (value, expected) in [
            (Variant::Bool(false), Ok(false)),
            (text(" true "), Ok(true)),
            (text("%flag%"), Ok(false)),
            (text("yes"), choices()),
            (text("TRUE"), choices()),
            (Variant::Int(1), choices()),
        ] {
            assert_eq!(read(&ENABLED, value.clone()), expected, "{value:?}");
        }
    }

    #[test]
    fn choice_field_reads_exact_wire_names_only_and_names_its_choices_otherwise() {
        let choices = || rejected(&PACE, ConfigReason::choices(["slow", "fast"]));
        for (value, expected) in [
            (text("fast"), Ok(Pace::Fast)),
            (text("Fast"), choices()),
            (text(" fast"), choices()),
            (text("%pace%"), choices()),
            (Variant::Int(0), choices()),
        ] {
            assert_eq!(read(&PACE, value.clone()), expected, "{value:?}");
        }
    }

    #[test]
    fn deferred_peek_skips_a_template_but_still_refuses_an_invalid_literal() {
        assert_eq!(
            (
                COUNT.peek(&stored("count", text("%big%")), Interpolation::Deferred),
                COUNT.peek(&stored("count", Variant::Int(3)), Interpolation::Deferred),
                COUNT.peek(&stored("count", Variant::Int(0)), Interpolation::Deferred),
            ),
            (
                Ok(None),
                Ok(Some(3)),
                rejected(&COUNT, ConfigReason::OutOfRange(COUNT.bounds)),
            ),
        );
    }

    #[test]
    fn peek_against_args_resolves_the_template_and_checks_the_result() {
        let args = args();

        assert_eq!(
            COUNT.peek(
                &stored("count", text("%big%")),
                Interpolation::Against(&args)
            ),
            rejected(&COUNT, ConfigReason::OutOfRange(COUNT.bounds)),
        );
    }
}
