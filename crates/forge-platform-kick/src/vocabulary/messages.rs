use forge_types::{MessageCatalog, MessageCode};

use crate::vocabulary::INTEGRATION_ID;

pub const STORED_SIGN_IN_UNREADABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "stored_sign_in_unreadable");

pub const KICK_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[STORED_SIGN_IN_UNREADABLE],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};
