use forge_types::{MessageCatalog, MessageCode};

use crate::vocabulary::INTEGRATION_ID;

pub const BACKEND_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "backend_unavailable");

pub const MIDI_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[BACKEND_UNAVAILABLE],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};
