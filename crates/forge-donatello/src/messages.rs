use forge_types::{MessageCatalog, MessageCode};

const INTEGRATION_ID: &str = "donatello";

pub const HEALTH_LAST_POLL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_poll");
pub const HEALTH_LAST_ERROR: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_error");
pub const HEALTH_LAST_DONATION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_donation");
pub const HEALTH_POLL_INTERVAL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_poll_interval");

pub const DONATELLO_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_LAST_POLL,
        HEALTH_LAST_ERROR,
        HEALTH_LAST_DONATION,
        HEALTH_POLL_INTERVAL,
    ],
    en_ftl: include_str!("../locales/en/messages.ftl"),
    uk_ftl: include_str!("../locales/uk/messages.ftl"),
};
