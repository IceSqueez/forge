use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::parties::{TwitchBroadcaster, TwitchUser};
use crate::vocabulary::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessageReceived {
    pub channel: String,
    pub user: ChatAuthor,
    pub message: String,
    pub badges: Vec<String>,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cheer: Option<ChatCheer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_channel: Option<ChatOrigin>,
}

impl TypedEvent for ChatMessageReceived {
    const KIND: &'static str = event_kinds::CHANNEL_CHAT_MESSAGE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAuthor {
    pub login: String,
    pub id: String,
    pub display_name: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCheer {
    pub bits: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatOrigin {
    pub login: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessageDeleted {
    pub message_id: String,
    pub target_user: TwitchUser,
}

impl TypedEvent for ChatMessageDeleted {
    const KIND: &'static str = event_kinds::CHANNEL_CHAT_MESSAGE_DELETE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatCleared {
    pub broadcaster: TwitchBroadcaster,
}

impl TypedEvent for ChatCleared {
    const KIND: &'static str = event_kinds::CHANNEL_CHAT_CLEAR;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSettingsUpdated {
    pub settings: ChatSettings,
}

impl TypedEvent for ChatSettingsUpdated {
    const KIND: &'static str = event_kinds::CHANNEL_CHAT_SETTINGS_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatSettings {
    pub emote_mode: bool,
    pub follower_mode: bool,
    pub follower_mode_duration_minutes: Option<i64>,
    pub slow_mode: bool,
    pub slow_mode_wait_time_seconds: Option<i64>,
    pub subscriber_mode: bool,
    pub unique_chat_mode: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedChatState {
    pub shared_chat: SharedChatSession,
    pub host: TwitchUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedChatSession {
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SharedChatBegan(pub SharedChatState);

impl TypedEvent for SharedChatBegan {
    const KIND: &'static str = event_kinds::CHANNEL_SHARED_CHAT_BEGIN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SharedChatUpdated(pub SharedChatState);

impl TypedEvent for SharedChatUpdated {
    const KIND: &'static str = event_kinds::CHANNEL_SHARED_CHAT_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SharedChatEnded(pub SharedChatState);

impl TypedEvent for SharedChatEnded {
    const KIND: &'static str = event_kinds::CHANNEL_SHARED_CHAT_END;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhisperReceived {
    pub user: WhisperSender,
    pub whisper: WhisperBody,
    pub whisper_thread_id: String,
}

impl TypedEvent for WhisperReceived {
    const KIND: &'static str = event_kinds::USER_WHISPER_MESSAGE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhisperSender {
    pub id: String,
    pub login: String,
    pub display_name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhisperBody {
    pub text: String,
}
