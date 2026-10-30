use forge_types::{MessageCatalog, MessageCode};

use crate::vocabulary::INTEGRATION_ID;

pub const STORED_SIGN_IN_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "stored_sign_in_unreadable");

pub const HEALTH_LIVE_CHAT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_live_chat");
pub const HEALTH_EVENTS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_events");
pub const HEALTH_API_CALLS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_api_calls");

pub const KICK_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_LIVE_CHAT,
        HEALTH_EVENTS,
        HEALTH_API_CALLS,
        STORED_SIGN_IN_UNREADABLE,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};
