use forge_types::{MessageCatalog, MessageCode};

use crate::vocabulary::INTEGRATION_ID;

pub const COMBO_ARG: &str = "combo";

pub const PORTAL_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "portal_unavailable");
pub const PERMISSION_DENIED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "permission_denied");
pub const COMBO_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "combo_rejected");
pub const ENGINE_STOPPED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "engine_stopped");
pub const BACKEND_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "backend_failed");

pub const HOTKEY_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        PORTAL_UNAVAILABLE,
        PERMISSION_DENIED,
        COMBO_REJECTED,
        ENGINE_STOPPED,
        BACKEND_FAILED,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};
