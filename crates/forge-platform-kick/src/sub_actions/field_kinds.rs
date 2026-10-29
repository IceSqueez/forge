use forge_registry::runner::SubActionConfig;
use forge_registry::{ConfigReason, FieldLiteral, FormField, TypedField, ValueBounds};
use forge_types::Variant;

const LIST_SEPARATOR: char = ',';

pub(super) fn stores_blank(config: &SubActionConfig, key: &str) -> bool {
    match config.get(key) {
        None => true,
        Some(Variant::String(text)) => text.trim().is_empty(),
        Some(_) => false,
    }
}

pub(super) fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

pub(super) struct WholeNumberField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub bounds: ValueBounds,
}

impl WholeNumberField {
    fn bounded(&self, number: u64) -> Result<u64, ConfigReason> {
        if i64::try_from(number).is_ok_and(|whole| self.bounds.admits_whole(whole)) {
            Ok(number)
        } else {
            Err(ConfigReason::OutOfRange(self.bounds))
        }
    }
}

impl TypedField for WholeNumberField {
    type Value = u64;

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
        Variant::String(String::new())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, u64>, ConfigReason> {
        let Variant::String(text) = value else {
            return Err(ConfigReason::NotText);
        };
        match FieldLiteral::of_text(text) {
            Some(template) => Ok(template),
            None => self.parse_text(text).map(FieldLiteral::Value),
        }
    }

    fn parse_text(&self, text: &str) -> Result<u64, ConfigReason> {
        let typed = text.trim();
        if typed.is_empty() {
            return Err(ConfigReason::Required);
        }
        let number = typed.parse::<u64>().map_err(|_| ConfigReason::NotANumber)?;
        self.bounded(number)
    }
}

pub(super) struct Blankable<F>(pub F);

impl<F: TypedField> TypedField for Blankable<F> {
    type Value = Option<F::Value>;

    fn key(&self) -> &'static str {
        self.0.key()
    }

    fn form_field(&self) -> FormField {
        self.0.form_field()
    }

    fn default_value(&self) -> Variant {
        Variant::String(String::new())
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
        Ok(match self.0.literal(value)? {
            FieldLiteral::Value(parsed) => FieldLiteral::Value(Some(parsed)),
            FieldLiteral::Template(text) => FieldLiteral::Template(text),
        })
    }

    fn parse_text(&self, text: &str) -> Result<Self::Value, ConfigReason> {
        if text.trim().is_empty() {
            Ok(None)
        } else {
            self.0.parse_text(text).map(Some)
        }
    }
}

pub(super) struct CommaListField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub default: &'static str,
    pub max_items: usize,
    pub required: bool,
}

impl TypedField for CommaListField {
    type Value = Vec<String>;

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

    fn literal<'a>(
        &self,
        value: &'a Variant,
    ) -> Result<FieldLiteral<'a, Vec<String>>, ConfigReason> {
        match value {
            Variant::String(text) => match FieldLiteral::of_text(text) {
                Some(template) => Ok(template),
                None => self.parse_text(text).map(FieldLiteral::Value),
            },
            _ => Err(ConfigReason::NotText),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Vec<String>, ConfigReason> {
        let items: Vec<String> = text
            .split(LIST_SEPARATOR)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect();
        if self.required && items.is_empty() {
            Err(ConfigReason::Required)
        } else if items.len() > self.max_items {
            Err(ConfigReason::CountOutOfRange(ValueBounds::at_most(
                self.max_items,
            )))
        } else {
            Ok(items)
        }
    }
}
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_registry::{ConfigProblem, Interpolation};
    use forge_types::ArgStack;

    use super::*;

    const KEY: &str = "amount";

    const AMOUNT: WholeNumberField = WholeNumberField {
        key: KEY,
        label: "Amount",
        placeholder: "",
        bounds: ValueBounds {
            min: Some(1),
            max: Some(5),
        },
    };

    const OPTIONAL_AMOUNT: Blankable<WholeNumberField> = Blankable(AMOUNT);

    const fn items(required: bool) -> CommaListField {
        CommaListField {
            key: KEY,
            label: "Items",
            placeholder: "",
            default: "",
            max_items: 3,
            required,
        }
    }

    fn stored(value: Variant) -> SubActionConfig {
        BTreeMap::from([(KEY.to_owned(), value)])
    }

    fn text(value: &str) -> SubActionConfig {
        stored(Variant::String(value.to_owned()))
    }

    fn rejected(reason: ConfigReason) -> ConfigProblem {
        ConfigProblem::field(KEY, reason)
    }

    fn args() -> ArgStack {
        ArgStack::new()
            .set("three".to_owned(), Variant::String(" 3 ".to_owned()))
            .set("word".to_owned(), Variant::String("many".to_owned()))
            .set("blank".to_owned(), Variant::String("  ".to_owned()))
            .set("pair".to_owned(), Variant::String("x, y".to_owned()))
    }

    #[test]
    fn whole_number_reads_trimmed_digits_inside_its_bounds() {
        for (raw, expected) in [("1", 1), (" 5 ", 5), ("%three%", 3)] {
            assert_eq!(AMOUNT.read(&text(raw), &args()), Ok(expected), "{raw:?}");
        }
    }

    #[test]
    fn whole_number_rejects_blank_non_numeric_out_of_range_and_non_text_values() {
        let out_of_range = ConfigReason::OutOfRange(AMOUNT.bounds);
        for (label, config, reason) in [
            ("empty", text(""), ConfigReason::Required),
            ("whitespace", text(" \t "), ConfigReason::Required),
            ("missing key", BTreeMap::new(), ConfigReason::Required),
            ("word", text("lots"), ConfigReason::NotANumber),
            ("negative", text("-1"), ConfigReason::NotANumber),
            ("fraction", text("2.5"), ConfigReason::NotANumber),
            ("below min", text("0"), out_of_range.clone()),
            ("above max", text("6"), out_of_range),
            (
                "integer variant",
                stored(Variant::Int(3)),
                ConfigReason::NotText,
            ),
            ("template to blank", text("%blank%"), ConfigReason::Required),
            ("template to word", text("%word%"), ConfigReason::NotANumber),
        ] {
            assert_eq!(
                AMOUNT.read(&config, &args()),
                Err(rejected(reason)),
                "case: {label}"
            );
        }
    }

    #[test]
    fn whole_number_defers_a_template_at_edit_time() {
        assert_eq!(
            AMOUNT.peek(&text("%word%"), Interpolation::Deferred),
            Ok(None)
        );
    }

    #[test]
    fn blankable_keeps_current_when_stored_or_resolved_blank() {
        for (label, config) in [
            ("missing key", BTreeMap::new()),
            ("empty", text("")),
            ("whitespace", text("   ")),
            ("template to blank", text("%blank%")),
        ] {
            assert_eq!(
                OPTIONAL_AMOUNT.read(&config, &args()),
                Ok(None),
                "case: {label}"
            );
        }
    }

    #[test]
    fn blankable_parses_a_present_value_with_the_wrapped_field_rules() {
        for (label, config, expected) in [
            ("literal", text("4"), Ok(Some(4))),
            ("template", text("%three%"), Ok(Some(3))),
            (
                "word",
                text("lots"),
                Err(rejected(ConfigReason::NotANumber)),
            ),
            (
                "above max",
                text("9"),
                Err(rejected(ConfigReason::OutOfRange(AMOUNT.bounds))),
            ),
            (
                "integer variant",
                stored(Variant::Int(3)),
                Err(rejected(ConfigReason::NotText)),
            ),
        ] {
            assert_eq!(
                OPTIONAL_AMOUNT.read(&config, &args()),
                expected,
                "case: {label}"
            );
        }
    }

    #[test]
    fn comma_list_splits_trims_and_drops_empty_items() {
        for (raw, expected) in [
            ("a", vec!["a"]),
            (" a , b ,c ", vec!["a", "b", "c"]),
            ("a,,b,", vec!["a", "b"]),
            ("ґанок, ї", vec!["ґанок", "ї"]),
            ("%pair%", vec!["x", "y"]),
        ] {
            assert_eq!(
                items(true).read(&text(raw), &args()),
                Ok(expected.iter().map(|item| (*item).to_owned()).collect()),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn comma_list_rejects_over_capacity_non_text_and_required_empty_lists() {
        let over = ConfigReason::CountOutOfRange(ValueBounds::at_most(3));
        for (label, field, config, reason) in [
            (
                "required empty",
                items(true),
                text(""),
                ConfigReason::Required,
            ),
            (
                "required separators only",
                items(true),
                text(" , ,"),
                ConfigReason::Required,
            ),
            (
                "required missing key",
                items(true),
                BTreeMap::new(),
                ConfigReason::Required,
            ),
            (
                "required template to blank",
                items(true),
                text("%blank%"),
                ConfigReason::Required,
            ),
            ("one over capacity", items(false), text("a,b,c,d"), over),
            (
                "integer variant",
                items(false),
                stored(Variant::Int(1)),
                ConfigReason::NotText,
            ),
        ] {
            assert_eq!(
                field.read(&config, &args()),
                Err(rejected(reason)),
                "case: {label}"
            );
        }
    }

    #[test]
    fn comma_list_accepts_exactly_capacity_and_an_optional_empty_list() {
        assert_eq!(
            items(false).read(&text("a,b,c"), &args()).map(|l| l.len()),
            Ok(3)
        );
        assert_eq!(items(false).read(&text(" , "), &args()), Ok(Vec::new()));
    }

    #[test]
    fn stores_blank_is_true_only_for_a_missing_or_whitespace_string() {
        for (label, config, expected) in [
            ("missing key", BTreeMap::new(), true),
            ("empty", text(""), true),
            ("whitespace", text(" \t"), true),
            ("text", text("x"), false),
            ("template", text("%x%"), false),
            ("zero integer", stored(Variant::Int(0)), false),
            ("false bool", stored(Variant::Bool(false)), false),
        ] {
            assert_eq!(stores_blank(&config, KEY), expected, "case: {label}");
        }
    }
}
