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

#[cfg(test)]
mod tests {
    use forge_registry::ConfigProblem;
    use forge_types::{ArgStack, SubActionConfig};
    use serde_json::json;

    use super::*;

    const SETTINGS: JsonObjectField = JsonObjectField {
        key: "settings",
        label: "Settings",
        default: "{}",
    };

    fn read(value: Variant) -> Result<Map<String, Value>, ConfigProblem> {
        let args = ArgStack::new().set("user".to_owned(), Variant::String("ann".to_owned()));
        SETTINGS.read(
            &SubActionConfig::from([("settings".to_owned(), value)]),
            &args,
        )
    }

    fn object(value: Value) -> Map<String, Value> {
        let Value::Object(map) = value else {
            unreachable!()
        };
        map
    }

    #[test]
    fn reads_a_json_object_text_as_the_object_keeping_nulls_and_nesting() {
        assert_eq!(
            read(Variant::String(
                r#" {"a":null,"b":{"c":[1,"x"]}} "#.to_owned()
            )),
            Ok(object(json!({"a": null, "b": {"c": [1, "x"]}}))),
        );
    }

    #[test]
    fn keeps_variable_references_inside_the_object_verbatim() {
        assert_eq!(
            read(Variant::String(r#"{"text":"%user%"}"#.to_owned())),
            Ok(object(json!({"text": "%user%"}))),
        );
    }

    #[test]
    fn reads_an_absent_value_as_the_declared_default_object() {
        assert_eq!(
            SETTINGS.read(&SubActionConfig::new(), &ArgStack::new()),
            Ok(Map::new()),
        );
    }

    #[test]
    fn refuses_text_that_is_not_a_json_object_as_an_invalid_format() {
        for text in ["[1,2]", "5", r#""str""#, "null", "{not valid", "", "%user%"] {
            let problem = read(Variant::String(text.to_owned())).unwrap_err();
            assert_eq!(problem.field.as_deref(), Some("settings"), "{text:?}");
            assert!(
                matches!(problem.reason, ConfigReason::InvalidFormat(_)),
                "{text:?} gave {:?}",
                problem.reason,
            );
        }
    }

    #[test]
    fn refuses_a_value_that_is_not_text() {
        for value in [
            Variant::Int(0),
            Variant::Bool(true),
            Variant::Object(Default::default()),
        ] {
            assert_eq!(
                read(value.clone()),
                Err(ConfigProblem::field("settings", ConfigReason::NotText)),
                "{value:?}",
            );
        }
    }
}
