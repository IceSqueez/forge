use forge_types::Variant;

use crate::config_problem::{ConfigReason, ValueBounds};
use crate::declared_config::{FieldLiteral, TypedField};
use crate::declared_fields::{TextEditor, TextField};
use crate::form::FormField;

const LIST_SEPARATORS: [char; 2] = ['\n', ','];

pub struct BoundedTextField {
    pub key: &'static str,
    pub label: &'static str,
    pub editor: TextEditor,
    pub default: &'static str,
    pub required: bool,
    pub length: ValueBounds,
}

impl BoundedTextField {
    const fn as_text(&self) -> TextField {
        TextField {
            key: self.key,
            label: self.label,
            editor: self.editor,
            default: self.default,
            required: self.required,
        }
    }
}

impl TypedField for BoundedTextField {
    type Value = String;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        self.as_text().form_field()
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, String>, ConfigReason> {
        match self.as_text().literal(value)? {
            FieldLiteral::Value(text) => self.parse_text(&text).map(FieldLiteral::Value),
            template @ FieldLiteral::Template(_) => Ok(template),
        }
    }

    fn parse_text(&self, text: &str) -> Result<String, ConfigReason> {
        let text = self.as_text().parse_text(text)?;
        if within_length(&text, self.length) {
            Ok(text)
        } else {
            Err(ConfigReason::LengthOutOfRange(self.length))
        }
    }
}

pub struct SeparatedListField {
    pub key: &'static str,
    pub label: &'static str,
    pub default: &'static str,
    pub count: ValueBounds,
    pub item_length: ValueBounds,
}

impl TypedField for SeparatedListField {
    type Value = Vec<String>;

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
    ) -> Result<FieldLiteral<'a, Vec<String>>, ConfigReason> {
        let Variant::String(text) = value else {
            return Err(ConfigReason::NotText);
        };
        match FieldLiteral::of_text(text) {
            Some(template) => Ok(template),
            None => self.parse_text(text).map(FieldLiteral::Value),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Vec<String>, ConfigReason> {
        let items: Vec<String> = text
            .split(LIST_SEPARATORS)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect();
        if !items
            .iter()
            .all(|item| within_length(item, self.item_length))
        {
            return Err(ConfigReason::LengthOutOfRange(self.item_length));
        }
        let count = i64::try_from(items.len()).unwrap_or(i64::MAX);
        if !self.count.admits_whole(count) {
            return Err(ConfigReason::CountOutOfRange(self.count));
        }
        Ok(items)
    }
}

fn within_length(text: &str, length: ValueBounds) -> bool {
    length.admits_whole(i64::try_from(text.chars().count()).unwrap_or(i64::MAX))
}
