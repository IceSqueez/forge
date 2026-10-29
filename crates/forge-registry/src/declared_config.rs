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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_problem::ValueBounds;
    use crate::declared_fields::{IntegerEditor, IntegerField, TextEditor, TextField};

    const TITLE: TextField = TextField {
        key: "title",
        label: "Title",
        editor: TextEditor::Line { placeholder: "" },
        default: "untitled",
        required: true,
    };
    const LIMIT: IntegerField = IntegerField {
        key: "limit",
        label: "Limit",
        editor: IntegerEditor::Number,
        default: 3,
        bounds: ValueBounds {
            min: Some(1),
            max: Some(5),
        },
    };
    const SPEC: ConfigSpec = ConfigSpec::new(&[&TITLE, &LIMIT]);

    fn config(entries: &[(&str, Variant)]) -> SubActionConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    #[test]
    fn default_config_holds_every_declared_field_default() {
        assert_eq!(
            SPEC.default_config(),
            config(&[("title", text("untitled")), ("limit", Variant::Int(3))])
        );
    }

    #[test]
    fn default_config_passes_its_own_validation() {
        assert_eq!(
            SPEC.validate(&SPEC.default_config(), Interpolation::Deferred),
            Ok(())
        );
    }

    #[test]
    fn form_fields_follow_declaration_order() {
        let keys: Vec<&str> = SPEC
            .form_fields()
            .into_iter()
            .map(|field| match field {
                FormField::Text { key, .. } | FormField::Integer { key, .. } => key,
                other => panic!("unexpected form field {other:?}"),
            })
            .collect();

        assert_eq!(keys, ["title", "limit"]);
    }

    #[test]
    fn validate_reports_the_first_invalid_field_in_declaration_order() {
        let both_invalid = config(&[("title", text(" ")), ("limit", Variant::Int(9))]);

        assert_eq!(
            SPEC.validate(&both_invalid, Interpolation::Deferred),
            Err(ConfigProblem::field("title", ConfigReason::Required))
        );
    }

    #[test]
    fn validate_checks_absent_keys_against_their_defaults() {
        assert_eq!(
            SPEC.validate(
                &config(&[("limit", Variant::Int(5))]),
                Interpolation::Deferred
            ),
            Ok(())
        );
    }

    #[test]
    fn deferred_validation_skips_a_template_that_the_run_time_check_refuses() {
        let templated = config(&[("limit", text("%size%"))]);
        let args = ArgStack::new().set("size".to_owned(), Variant::Int(6));

        assert_eq!(
            (
                SPEC.validate(&templated, Interpolation::Deferred),
                SPEC.validate(&templated, Interpolation::Against(&args)),
            ),
            (
                Ok(()),
                Err(ConfigProblem::field(
                    "limit",
                    ConfigReason::OutOfRange(LIMIT.bounds)
                )),
            )
        );
    }

    #[test]
    fn literal_of_text_is_a_template_only_when_the_text_references_a_variable() {
        for (raw, is_template) in [
            ("%name%", true),
            ("hi %name%!", true),
            ("plain", false),
            ("100%", false),
            ("%%", false),
            ("% spaced %", false),
        ] {
            assert_eq!(
                matches!(
                    FieldLiteral::<()>::of_text(raw),
                    Some(FieldLiteral::Template(text)) if text == raw
                ),
                is_template,
                "{raw:?}"
            );
        }
    }
}
