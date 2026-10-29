use std::borrow::Cow;

use forge_types::{ArgStack, SubActionConfig, Variant, variable_references};

use crate::config_problem::{ConfigProblem, ConfigReason};
use crate::form::FormField;

#[derive(Clone, Copy)]
pub enum Interpolation<'a> {
    Deferred,
    Against(&'a ArgStack),
}

pub enum FieldLiteral<'a, T> {
    Value(T),
    Template(&'a str),
}

impl<'a, T> FieldLiteral<'a, T> {
    pub fn of_text(text: &'a str) -> Option<Self> {
        variable_references(text)
            .next()
            .map(|_| Self::Template(text))
    }
}

pub trait TypedField: Sync {
    type Value;

    fn key(&self) -> &'static str;
    fn form_field(&self) -> FormField;
    fn default_value(&self) -> Variant;
    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Self::Value>, ConfigReason>;
    fn parse_text(&self, text: &str) -> Result<Self::Value, ConfigReason>;

    fn read(
        &self,
        config: &SubActionConfig,
        args: &ArgStack,
    ) -> Result<Self::Value, ConfigProblem> {
        let value = stored_or_default(self, config);
        match self
            .literal(&value)
            .map_err(|reason| problem(self, reason))?
        {
            FieldLiteral::Value(parsed) => Ok(parsed),
            FieldLiteral::Template(text) => self
                .parse_text(&args.interpolate(text))
                .map_err(|reason| problem(self, reason)),
        }
    }

    fn peek(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<Option<Self::Value>, ConfigProblem> {
        let Interpolation::Against(args) = interpolation else {
            let value = stored_or_default(self, config);
            return match self
                .literal(&value)
                .map_err(|reason| problem(self, reason))?
            {
                FieldLiteral::Value(parsed) => Ok(Some(parsed)),
                FieldLiteral::Template(_) => Ok(None),
            };
        };
        self.read(config, args).map(Some)
    }
}

fn stored_or_default<'c, F: TypedField + ?Sized>(
    field: &F,
    config: &'c SubActionConfig,
) -> Cow<'c, Variant> {
    config
        .get(field.key())
        .map_or_else(|| Cow::Owned(field.default_value()), Cow::Borrowed)
}

fn problem<F: TypedField + ?Sized>(field: &F, reason: ConfigReason) -> ConfigProblem {
    ConfigProblem::field(field.key(), reason)
}

pub trait DeclaredField: Sync {
    fn key(&self) -> &'static str;
    fn form_field(&self) -> FormField;
    fn default_value(&self) -> Variant;
    fn check(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem>;
}

impl<F: TypedField> DeclaredField for F {
    fn key(&self) -> &'static str {
        TypedField::key(self)
    }

    fn form_field(&self) -> FormField {
        TypedField::form_field(self)
    }

    fn default_value(&self) -> Variant {
        TypedField::default_value(self)
    }

    fn check(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        self.peek(config, interpolation).map(|_| ())
    }
}

#[derive(Clone, Copy)]
pub struct ConfigSpec {
    fields: &'static [&'static dyn DeclaredField],
}

impl ConfigSpec {
    pub const fn new(fields: &'static [&'static dyn DeclaredField]) -> Self {
        Self { fields }
    }

    pub fn fields(&self) -> impl Iterator<Item = &'static dyn DeclaredField> {
        self.fields.iter().copied()
    }

    pub fn default_config(&self) -> SubActionConfig {
        self.fields()
            .map(|field| (field.key().to_owned(), field.default_value()))
            .collect()
    }

    pub fn form_fields(&self) -> Vec<FormField> {
        self.fields().map(DeclaredField::form_field).collect()
    }

    pub fn validate(
        &self,
        config: &SubActionConfig,
        interpolation: Interpolation<'_>,
    ) -> Result<(), ConfigProblem> {
        self.fields()
            .try_for_each(|field| field.check(config, interpolation))
    }
}
