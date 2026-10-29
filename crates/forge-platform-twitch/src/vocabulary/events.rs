use forge_events::TypedEvent;
use forge_types::PlatformId;
use serde::{Deserialize, Serialize};

use super::event_kinds;
use crate::bot_account::{BotModerator, BotStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformReauthRequired {
    pub platform_id: PlatformId,
}

impl TypedEvent for PlatformReauthRequired {
    const KIND: &'static str = event_kinds::PLATFORM_REAUTH_REQUIRED;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotStatusTag {
    NotConnected,
    Connected,
    ReauthRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotStatusChanged {
    pub status: BotStatusTag,
    pub login: Option<String>,
    pub moderator: BotModerator,
}

impl BotStatusChanged {
    pub fn new(status: &BotStatus, moderator: BotModerator) -> Self {
        let tag = match status {
            BotStatus::NotConnected => BotStatusTag::NotConnected,
            BotStatus::Connected { .. } => BotStatusTag::Connected,
            BotStatus::ReauthRequired { .. } => BotStatusTag::ReauthRequired,
        };
        Self {
            status: tag,
            login: status.login().map(str::to_owned),
            moderator,
        }
    }

    pub fn bot_status(&self) -> Option<BotStatus> {
        match (self.status, &self.login) {
            (BotStatusTag::NotConnected, _) => Some(BotStatus::NotConnected),
            (BotStatusTag::Connected, Some(login)) => Some(BotStatus::Connected {
                login: login.clone(),
            }),
            (BotStatusTag::ReauthRequired, Some(login)) => Some(BotStatus::ReauthRequired {
                login: login.clone(),
            }),
            (BotStatusTag::Connected | BotStatusTag::ReauthRequired, None) => None,
        }
    }
}

impl TypedEvent for BotStatusChanged {
    const KIND: &'static str = event_kinds::BOT_STATUS_CHANGED;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackLevel {
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SenderFallback {
    pub level: FallbackLevel,
    pub operation: String,
    pub reason: String,
}

impl TypedEvent for SenderFallback {
    const KIND: &'static str = event_kinds::SENDER_FALLBACK;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventSubSubscriptionCreated {
    #[serde(rename = "type")]
    pub subscription_type: String,
    pub subscription_id: Option<String>,
    pub condition: serde_json::Value,
}

impl TypedEvent for EventSubSubscriptionCreated {
    const KIND: &'static str = event_kinds::EVENTSUB_SUBSCRIPTION_CREATED;
}
