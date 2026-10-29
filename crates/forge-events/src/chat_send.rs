use serde::{Deserialize, Serialize};

use crate::TypedEvent;

pub const SENT_AS_FIELD: &str = "sent_as";

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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSendRequest {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whisper_to_login: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
}

impl TypedEvent for ChatSendRequest {
    const KIND: &'static str = "chat.send.request";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSent {
    pub channel: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_as: Option<ActingSender>,
}

impl TypedEvent for ChatSent {
    const KIND: &'static str = "chat.send";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSendFailed {
    pub channel: String,
    pub error: String,
}

impl TypedEvent for ChatSendFailed {
    const KIND: &'static str = "chat.send.failed";
}
