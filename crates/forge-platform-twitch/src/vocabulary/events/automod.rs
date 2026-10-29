use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::parties::TwitchUser;
use crate::vocabulary::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomodMessageHeld {
    pub automod: HeldMessage,
    pub reason: String,
    pub blocked_term: Option<BlockedTermMatch>,
    pub user: TwitchUser,
    pub message_text: String,
}

impl TypedEvent for AutomodMessageHeld {
    const KIND: &'static str = event_kinds::AUTOMOD_MESSAGE_HOLD;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeldMessage {
    pub message_id: String,
    pub category: Option<String>,
    pub level: Option<i64>,
    pub held_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockedTermMatch {
    pub terms_found: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomodMessageUpdated {
    pub automod: ReviewedMessage,
    pub reason: String,
    pub blocked_term: Option<BlockedTermMatch>,
    pub user: TwitchUser,
    pub moderator: TwitchUser,
    pub message_text: String,
}

impl TypedEvent for AutomodMessageUpdated {
    const KIND: &'static str = event_kinds::AUTOMOD_MESSAGE_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedMessage {
    pub message_id: String,
    pub status: String,
    pub category: Option<String>,
    pub level: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomodSettingsUpdated {
    pub moderator: TwitchUser,
    pub overall_level: Option<i64>,
}

impl TypedEvent for AutomodSettingsUpdated {
    const KIND: &'static str = event_kinds::AUTOMOD_SETTINGS_UPDATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomodTermsUpdated {
    pub moderator: TwitchUser,
    pub action: String,
    pub terms: Vec<serde_json::Value>,
}

impl TypedEvent for AutomodTermsUpdated {
    const KIND: &'static str = event_kinds::AUTOMOD_TERMS_UPDATE;
}
