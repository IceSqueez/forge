use forge_registry::{ConfigReason, FieldLiteral, FormField, TypedField};
use forge_types::Variant;
use serde_json::{Map, Value};

pub(crate) struct JsonObjectField {
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) default: &'static str,
}

impl TypedField for JsonObjectField {
    type Value = Map<String, Value>;

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
            Variant::String(text) => self.parse_text(text).map(FieldLiteral::Value),
            _ => Err(ConfigReason::NotText),
        }
    }

    fn parse_text(&self, text: &str) -> Result<Self::Value, ConfigReason> {
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(settings)) => Ok(settings),
            Ok(_) => Err(ConfigReason::InvalidFormat(
                "expected a JSON object".to_owned(),
            )),
            Err(error) => Err(ConfigReason::InvalidFormat(format!(
                "not valid JSON ({error})"
            ))),
        }
    }
}
