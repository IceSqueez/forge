use forge_registry::JsonObjectField;

pub(super) const EVENT_NAME_KEY: &str = "event_name";
pub(super) const EMPTY_JSON_OBJECT: &str = "{}";

pub(super) const PAYLOAD: JsonObjectField = JsonObjectField {
    key: "payload",
    label: "Payload (JSON object)",
    default: EMPTY_JSON_OBJECT,
};
