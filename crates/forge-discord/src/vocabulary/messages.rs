use forge_types::{MessageCatalog, MessageCode};

use crate::vocabulary::INTEGRATION_ID;

pub const HEALTH_WEBHOOK_LATENCY: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_webhook_latency");
pub const HEALTH_RATE_LIMIT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_rate_limit");
pub const HEALTH_LAST_SEND: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_send");
pub const HEALTH_ERRORS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_errors");

pub const DISCORD_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_WEBHOOK_LATENCY,
        HEALTH_RATE_LIMIT,
        HEALTH_LAST_SEND,
        HEALTH_ERRORS,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};
