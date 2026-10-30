use forge_types::MessageCode;

use crate::vocabulary::INTEGRATION_ID;

pub(crate) const ACTIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_active");
pub(crate) const RECENT_TRIGGERS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_recent_triggers");
pub(crate) const REGISTERED_HOTKEYS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_registered_hotkeys");
