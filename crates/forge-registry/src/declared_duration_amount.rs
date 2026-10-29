use forge_types::Variant;

use crate::config_problem::{ConfigReason, ValueBounds};
use crate::declared_config::{FieldLiteral, TypedField};
use crate::duration_bounds::DurationBounds;
use crate::form::FormField;

pub struct DurationAmountField {
    pub key: &'static str,
    pub label: &'static str,
    pub default: i64,
    pub bounds: DurationBounds,
}

impl DurationAmountField {
    fn allowed(&self) -> ValueBounds {
        ValueBounds::between(self.bounds.min, self.bounds.max)
    }

    fn bounded(&self, stored: i64) -> Result<u64, ConfigReason> {
        match u64::try_from(stored) {
            Ok(amount) if self.allowed().admits_whole(stored) => Ok(amount),
            _ => Err(ConfigReason::OutOfRange(self.allowed())),
        }
    }
}

impl TypedField for DurationAmountField {
    type Value = u64;

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

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, u64>, ConfigReason> {
        match value {
            Variant::Int(stored) => self.bounded(*stored).map(FieldLiteral::Value),
            Variant::String(text) => match FieldLiteral::of_text(text) {
                Some(template) => Ok(template),
                None => self.parse_text(text).map(FieldLiteral::Value),
            },
            _ => Err(ConfigReason::NotANumber),
        }
    }

    fn parse_text(&self, text: &str) -> Result<u64, ConfigReason> {
        let typed = text.trim();
        if typed.is_empty() {
            return Err(ConfigReason::Required);
        }
        let stored = typed.parse::<i64>().map_err(|_| ConfigReason::NotANumber)?;
        self.bounded(stored)
    }
}
