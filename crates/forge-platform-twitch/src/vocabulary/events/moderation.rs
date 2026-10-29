use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::parties::{TwitchUser, UserRef};
use crate::vocabulary::event_kinds;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserBanned {
    pub user: TwitchUser,
    pub moderator: TwitchUser,
    pub reason: String,
    pub banned_at: String,
    pub ends_at: Option<String>,
    pub is_permanent: bool,
}

impl TypedEvent for UserBanned {
    const KIND: &'static str = event_kinds::CHANNEL_BAN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserUnbanned {
    pub user: TwitchUser,
    pub moderator: TwitchUser,
}

impl TypedEvent for UserUnbanned {
    const KIND: &'static str = event_kinds::CHANNEL_UNBAN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeratorAdded(pub UserRef);

impl TypedEvent for ModeratorAdded {
    const KIND: &'static str = event_kinds::CHANNEL_MODERATOR_ADD;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeratorRemoved(pub UserRef);

impl TypedEvent for ModeratorRemoved {
    const KIND: &'static str = event_kinds::CHANNEL_MODERATOR_REMOVE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VipAdded(pub UserRef);

impl TypedEvent for VipAdded {
    const KIND: &'static str = event_kinds::CHANNEL_VIP_ADD;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VipRemoved(pub UserRef);

impl TypedEvent for VipRemoved {
    const KIND: &'static str = event_kinds::CHANNEL_VIP_REMOVE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnbanRequestCreated {
    pub id: String,
    pub user: TwitchUser,
    pub reason_text: String,
}

impl TypedEvent for UnbanRequestCreated {
    const KIND: &'static str = event_kinds::CHANNEL_UNBAN_REQUEST_CREATE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnbanRequestResolved {
    pub id: String,
    pub user: TwitchUser,
    pub status: String,
    pub moderator: TwitchUser,
    pub resolution_text: String,
}

impl TypedEvent for UnbanRequestResolved {
    const KIND: &'static str = event_kinds::CHANNEL_UNBAN_REQUEST_RESOLVE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShieldModeBegan {
    pub moderator: TwitchUser,
    pub started_at: String,
}

impl TypedEvent for ShieldModeBegan {
    const KIND: &'static str = event_kinds::CHANNEL_SHIELD_MODE_BEGIN;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShieldModeEnded {
    pub moderator: TwitchUser,
    pub ended_at: String,
}

impl TypedEvent for ShieldModeEnded {
    const KIND: &'static str = event_kinds::CHANNEL_SHIELD_MODE_END;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuspiciousUserMessaged {
    pub user: TwitchUser,
    pub low_trust_status: String,
    pub message_text: String,
}

impl TypedEvent for SuspiciousUserMessaged {
    const KIND: &'static str = event_kinds::CHANNEL_SUSPICIOUS_USER_MESSAGE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WarningAcknowledged(pub UserRef);

impl TypedEvent for WarningAcknowledged {
    const KIND: &'static str = event_kinds::CHANNEL_WARNING_ACKNOWLEDGE;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarningSent {
    pub user: TwitchUser,
    pub moderator: TwitchUser,
    pub reason: String,
    pub chat_rules_cited: Vec<serde_json::Value>,
}

impl TypedEvent for WarningSent {
    const KIND: &'static str = event_kinds::CHANNEL_WARNING_SEND;
}
