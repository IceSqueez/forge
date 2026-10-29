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
