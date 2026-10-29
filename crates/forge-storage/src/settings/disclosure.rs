use forge_types::redaction::STAMP;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingDisclosure {
    Verbatim,
    Kind,
    Presence,
    Withheld,
}

impl SettingDisclosure {
    pub fn render(self, value: &str) -> Option<String> {
        match self {
            Self::Verbatim => Some(value.to_owned()),
            Self::Kind => Some(shape_of(value)),
            Self::Presence => Some(if value.is_empty() { "unset" } else { "set" }.to_owned()),
            Self::Withheld => None,
        }
    }
}

fn shape_of(value: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(value) {
        Ok(serde_json::Value::Array(items)) => format!("list({})", items.len()),
        Ok(serde_json::Value::Object(fields)) => format!("map({})", fields.len()),
        _ => format!("{STAMP} len={}>", value.chars().count()),
    }
}
