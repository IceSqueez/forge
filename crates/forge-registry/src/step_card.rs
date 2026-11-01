use forge_types::{MessageRef, SubActionConfig, Variant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepCardDetail {
    Text(String),
    Message(MessageRef),
    OptionLabel {
        options_key: &'static str,
        value: String,
    },
}

pub fn card_text<'a>(config: &'a SubActionConfig, key: &str) -> Option<&'a str> {
    match config.get(key) {
        Some(Variant::String(text)) => Some(text),
        Some(_) => Some(""),
        None => None,
    }
}

pub fn card_int(config: &SubActionConfig, key: &str) -> Option<i64> {
    match config.get(key) {
        Some(Variant::Int(number)) => Some(*number),
        Some(_) => Some(0),
        None => None,
    }
}

pub fn card_scalar(config: &SubActionConfig, key: &str) -> String {
    match config.get(key) {
        Some(value @ (Variant::Array(_) | Variant::Object(_))) => value.to_plain_json().to_string(),
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

pub fn card_variable(name: &str) -> String {
    format!("%{}%", name.trim_matches('%'))
}
