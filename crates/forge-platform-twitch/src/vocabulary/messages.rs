use forge_types::{MessageCatalog, MessageCode};

use crate::vocabulary::PLATFORM_ID;

pub const HEALTH_CHAT: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_chat");
pub const HEALTH_EVENTSUB: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_eventsub");
pub const HEALTH_API_CALLS: MessageCode = MessageCode::from_static(PLATFORM_ID, "health_api_calls");

pub const TWITCH_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: PLATFORM_ID,
    codes: &[HEALTH_CHAT, HEALTH_EVENTSUB, HEALTH_API_CALLS],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};
