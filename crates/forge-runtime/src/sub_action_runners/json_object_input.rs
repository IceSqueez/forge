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
