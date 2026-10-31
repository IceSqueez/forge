use forge_storage::StorageError;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub(crate) fn parse_label<T: DeserializeOwned>(label: &str) -> Result<T, serde_json::Error> {
    serde_json::from_value(Value::String(label.to_owned()))
}

pub(crate) fn wire_label<T: Serialize>(value: &T) -> Result<String, StorageError> {
    Ok(
        match serde_json::to_value(value).map_err(StorageError::Serialization)? {
            Value::String(label) => label,
            other => other.to_string(),
        },
    )
}
