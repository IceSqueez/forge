use forge_types::{ArgStack, SubActionConfig, Variant};

use crate::config_problem::{ConfigProblem, ConfigReason, ValueBounds};
use crate::declared_config::{DeclaredField, Interpolation, TypedField};
use crate::declared_fields::{IntegerEditor, IntegerField};
use crate::form::{AmountUnit, FormField, UnitAmountBounds};

pub struct UnitAmountField {
    pub key: &'static str,
    pub unit_key: &'static str,
    pub label: &'static str,
    pub bounds: UnitAmountBounds,
    pub default_amount: i64,
    pub default_unit: &'static str,
}

impl UnitAmountField {
    fn amount(&self) -> IntegerField {
        IntegerField {
            key: self.key,
            label: self.label,
            editor: IntegerEditor::Number,
            default: self.default_amount,
            bounds: ValueBounds::UNBOUNDED,
        }
    }

    fn unit(&self, config: &SubActionConfig) -> Result<&'static AmountUnit, ConfigProblem> {
        let stored = config.get(self.unit_key);
        let named = match stored {
            None => Some(self.default_unit),
            Some(value) => value.as_str(),
        };
        named
            .and_then(|name| self.bounds.units.iter().find(|unit| unit.value == name))
            .ok_or_else(|| {
                ConfigProblem::field(
                    self.unit_key,
                    ConfigReason::choices(self.bounds.units.iter().map(|unit| unit.value)),
                )
            })
    }

    fn base_units(&self, amount: i64, unit: &AmountUnit) -> Result<u64, ConfigProblem> {
        let range = self.bounds.range_for(unit.value);
        match u64::try_from(amount) {
            Ok(whole) if range.contains(&amount) => Ok(whole.saturating_mul(unit.base_units)),
            _ => Err(ConfigProblem::field(
                self.key,
                ConfigReason::OutOfRange(ValueBounds::between(*range.start(), *range.end())),
            )),
        }
    }

    pub fn read(&self, config: &SubActionConfig, args: &ArgStack) -> Result<u64, ConfigProblem> {
        let unit = self.unit(config)?;
        let amount = self.amount().read(config, args)?;
        self.base_units(amount, unit)
    }

    pub fn peek(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<Option<u64>, ConfigProblem> {
        let unit = self.unit(config)?;
        self.amount()
            .peek(config, interpolation)?
            .map(|amount| self.base_units(amount, unit))
            .transpose()
    }
}

impl DeclaredField for UnitAmountField {
    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::UnitAmount {
            key: self.key,
            label: self.label,
            unit_key: self.unit_key,
            bounds: self.bounds,
        }
    }

    fn write_default(&self, config: &mut SubActionConfig) {
        config.insert(self.key.to_owned(), Variant::Int(self.default_amount));
        config.insert(
            self.unit_key.to_owned(),
            Variant::String(self.default_unit.to_owned()),
        );
    }

    fn check(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        self.peek(config, interpolation).map(|_| ())
    }
}
