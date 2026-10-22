use serde::{Deserialize, Serialize};

use crate::PlatformError;

pub const SENDER_FIELD: &str = "sender";

pub const SENT_AS_FIELD: &str = "sent_as";

const BOT_SENDER_FEATURE: &str = "chat.bot_sender";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderPreference {
    #[default]
    Default,
    Streamer,
    #[serde(rename = "bot")]
    BotOnly,
}

impl SenderPreference {
    pub const ALL: [SenderPreference; 3] = [Self::Default, Self::Streamer, Self::BotOnly];

    pub const WIRE_NAMES: &'static [&'static str] = &["default", "streamer", "bot"];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Streamer => "streamer",
            Self::BotOnly => "bot",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sender| sender.as_str() == raw)
    }

    pub fn from_payload(payload: &serde_json::Value) -> Result<Self, String> {
        match payload.get(SENDER_FIELD) {
            None | Some(serde_json::Value::Null) => Ok(Self::Default),
            Some(serde_json::Value::String(raw)) => {
                Self::parse(raw).ok_or_else(|| unknown_sender_reason(raw))
            }
            Some(other) => Err(unknown_sender_reason(&other.to_string())),
        }
    }
}

pub fn unknown_sender_reason(raw: &str) -> String {
    let valid = SenderPreference::WIRE_NAMES.join(", ");
    format!("unknown sender \"{raw}\"; valid senders: {valid}")
}

pub fn bot_sender_unsupported() -> PlatformError {
    PlatformError::Unsupported {
        feature: BOT_SENDER_FEATURE.to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenderRole {
    Streamer,
    Bot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActingSender {
    pub role: SenderRole,
    pub login: String,
}

impl ActingSender {
    pub fn insert_into(&self, payload: &mut serde_json::Value) {
        if let (Some(fields), Ok(sent_as)) = (payload.as_object_mut(), serde_json::to_value(self)) {
            fields.insert(SENT_AS_FIELD.to_owned(), sent_as);
        }
    }
}
