use forge_types::{TriggerConfig, Variant};

pub(super) fn config_int(config: &TriggerConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(n)) => Some(*n),
        _ => None,
    }
}

pub(super) fn config_filter_text<'a>(config: &'a TriggerConfig, key: &str) -> Option<&'a str> {
    match config.get(key) {
        Some(Variant::String(text)) if !text.is_empty() => Some(text),
        _ => None,
    }
}
