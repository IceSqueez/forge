use std::collections::BTreeMap;

use forge_registry::{ConfigReason, RegistryError};
use forge_types::{SubActionConfig, Variant};

pub(super) const EMPTY_JSON_OBJECT: &str = "{}";

pub(super) fn read_json_object(
    config: &SubActionConfig,
    key: &str,
) -> Result<BTreeMap<String, Variant>, String> {
    json_object(config, key).map_err(|reason| format!("{key}: {reason}"))
}

pub(super) fn validate_json_object(
    config: &SubActionConfig,
    key: &str,
) -> Result<(), RegistryError> {
    json_object(config, key)
        .map(|_| ())
        .map_err(|reason| RegistryError::invalid_field(key, ConfigReason::InvalidFormat(reason)))
}

pub(super) fn plain_json_object(map: &BTreeMap<String, Variant>) -> serde_json::Value {
    serde_json::Value::Object(
        map.iter()
            .map(|(k, v)| (k.clone(), v.to_plain_json()))
            .collect(),
    )
}

fn json_object(config: &SubActionConfig, key: &str) -> Result<BTreeMap<String, Variant>, String> {
    match config.get(key) {
        None => Ok(BTreeMap::new()),
        Some(Variant::Object(map)) => Ok(map.clone()),
        Some(Variant::String(text)) if text.trim().is_empty() => Ok(BTreeMap::new()),
        Some(Variant::String(text)) => parse_object(text),
        Some(_) => Err(not_an_object()),
    }
}

fn parse_object(text: &str) -> Result<BTreeMap<String, Variant>, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("not valid JSON ({e})"))?;
    match Variant::from_plain_json(json) {
        Ok(Variant::Object(map)) => Ok(map),
        Ok(_) => Err(not_an_object()),
        Err(e) => Err(format!("not convertible ({e})")),
    }
}

fn not_an_object() -> String {
    "not a JSON object".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_registry::ConfigProblem;

    const KEY: &str = "payload";

    fn config(value: Option<Variant>) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        if let Some(value) = value {
            c.insert(KEY.to_owned(), value);
        }
        c
    }

    fn text(s: &str) -> Option<Variant> {
        Some(Variant::String(s.to_owned()))
    }

    #[test]
    fn accepted_inputs_read_as_the_expected_map() {
        let legacy = BTreeMap::from([("n".to_owned(), Variant::Int(1))]);
        for (input, expected) in [
            (
                text(r#"{"a": "x", "n": [1, 2.5, true], "o": {"k": "v"}}"#),
                BTreeMap::from([
                    ("a".to_owned(), Variant::String("x".to_owned())),
                    (
                        "n".to_owned(),
                        Variant::Array(vec![
                            Variant::Int(1),
                            Variant::Float(2.5),
                            Variant::Bool(true),
                        ]),
                    ),
                    (
                        "o".to_owned(),
                        Variant::Object(BTreeMap::from([(
                            "k".to_owned(),
                            Variant::String("v".to_owned()),
                        )])),
                    ),
                ]),
            ),
            (text("{}"), BTreeMap::new()),
            (text(""), BTreeMap::new()),
            (text("  \n "), BTreeMap::new()),
            (None, BTreeMap::new()),
            (Some(Variant::Object(legacy.clone())), legacy),
        ] {
            assert_eq!(
                read_json_object(&config(input.clone()), KEY).unwrap(),
                expected,
                "{input:?}"
            );
        }
    }

    #[test]
    fn rejected_inputs_fail_read_naming_the_field() {
        for input in rejected_inputs() {
            let err = read_json_object(&config(input.clone()), KEY).unwrap_err();
            assert!(err.starts_with(KEY), "{input:?}: {err}");
        }
    }

    #[test]
    fn rejected_inputs_fail_validation_as_invalid_format_on_the_field() {
        for input in rejected_inputs() {
            let err = validate_json_object(&config(input.clone()), KEY).unwrap_err();
            assert!(
                matches!(
                    &err,
                    RegistryError::InvalidConfig(ConfigProblem {
                        field: Some(field),
                        reason: ConfigReason::InvalidFormat(_),
                    }) if field == KEY
                ),
                "{input:?}: {err:?}"
            );
        }
    }

    #[test]
    fn plain_json_object_renders_untagged_values() {
        let map = BTreeMap::from([
            ("s".to_owned(), Variant::String("hi".to_owned())),
            (
                "list".to_owned(),
                Variant::Array(vec![Variant::Int(1), Variant::Bool(false)]),
            ),
        ]);
        assert_eq!(
            plain_json_object(&map),
            serde_json::json!({"s": "hi", "list": [1, false]})
        );
    }

    fn rejected_inputs() -> Vec<Option<Variant>> {
        vec![
            text("object{0}"),
            text("{\"a\": 1"),
            text("[1]"),
            text("null"),
            text("42"),
            text("\"str\""),
            text(r#"{"a": null}"#),
            Some(Variant::Int(1)),
            Some(Variant::Array(vec![])),
        ]
    }
}
