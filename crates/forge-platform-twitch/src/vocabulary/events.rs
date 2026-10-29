pub mod automod;
pub mod channel;
pub mod chat;
pub mod engagement;
pub mod moderation;
pub mod parties;
pub mod rewards;
pub mod support;

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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn bot_status_payload_carries_wire_status_login_and_moderator_for_every_combination() {
        let login = || "forgebot".to_owned();
        let statuses = [
            (BotStatus::NotConnected, "not_connected", json!(null)),
            (
                BotStatus::Connected { login: login() },
                "connected",
                json!("forgebot"),
            ),
            (
                BotStatus::ReauthRequired { login: login() },
                "reauth_required",
                json!("forgebot"),
            ),
        ];
        let moderators = [
            (BotModerator::Unknown, "unknown"),
            (BotModerator::Moderator, "moderator"),
            (BotModerator::NotModerator, "not_moderator"),
        ];
        for (status, wire_status, wire_login) in &statuses {
            for (moderator, wire_moderator) in moderators {
                let payload =
                    serde_json::to_value(BotStatusChanged::new(status, moderator)).unwrap();

                assert_eq!(
                    payload,
                    json!({"status": wire_status, "login": wire_login, "moderator": wire_moderator})
                );
            }
        }
    }

    #[test]
    fn bot_status_payload_round_trips_back_to_the_status() {
        let status = BotStatus::ReauthRequired {
            login: "forgebot".to_owned(),
        };
        let payload =
            serde_json::to_value(BotStatusChanged::new(&status, BotModerator::Unknown)).unwrap();

        let back: BotStatusChanged = serde_json::from_value(payload).unwrap();

        assert_eq!(back.bot_status(), Some(status));
    }

    #[test]
    fn bot_status_without_a_login_on_a_signed_in_tag_is_rejected() {
        let event = BotStatusChanged {
            status: BotStatusTag::Connected,
            login: None,
            moderator: BotModerator::Unknown,
        };

        assert_eq!(event.bot_status(), None);
    }

    #[test]
    fn sender_fallback_payload_is_level_operation_reason() {
        let payload = serde_json::to_value(SenderFallback {
            level: FallbackLevel::Warning,
            operation: "reply".to_owned(),
            reason: "bot offline".to_owned(),
        })
        .unwrap();

        assert_eq!(
            payload,
            json!({"level": "warning", "operation": "reply", "reason": "bot offline"})
        );
    }

    #[test]
    fn subscription_created_payload_renames_the_type_key_and_keeps_a_null_id() {
        let payload = serde_json::to_value(EventSubSubscriptionCreated {
            subscription_type: "stream.online".to_owned(),
            subscription_id: None,
            condition: json!({"broadcaster_user_id": "B"}),
        })
        .unwrap();

        assert_eq!(
            payload,
            json!({
                "type": "stream.online",
                "subscription_id": null,
                "condition": {"broadcaster_user_id": "B"},
            })
        );
    }

    #[test]
    fn reauth_required_payload_is_the_platform_id_only() {
        let payload = serde_json::to_value(PlatformReauthRequired {
            platform_id: PlatformId::Twitch,
        })
        .unwrap();

        assert_eq!(payload, json!({"platform_id": "twitch"}));
    }
}
