use forge_registry::{TextEditor, TextField};

use crate::account::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixError, HelixTransport};
use crate::vocabulary::config_keys;
use crate::vocabulary::messages;
use forge_types::MessageRef;

const SESSION_ID_TEMPLATE: &str = "%guest_star.session_id%";

pub(crate) const SESSION_ID: TextField = TextField {
    key: config_keys::SESSION_ID,
    label: "Session ID",
    editor: TextEditor::Line {
        placeholder: SESSION_ID_TEMPLATE,
    },
    default: SESSION_ID_TEMPLATE,
    required: true,
};

pub(crate) const GUEST_LOGIN: TextField = TextField {
    key: config_keys::TARGET_USER_LOGIN,
    label: "Guest Login",
    editor: TextEditor::Line {
        placeholder: "%user_login%",
    },
    default: "",
    required: true,
};

pub(crate) const SLOT_ID: TextField = TextField {
    key: config_keys::SLOT_ID,
    label: "Slot ID",
    editor: TextEditor::Line { placeholder: "1" },
    default: "",
    required: true,
};

pub(crate) fn first_missing(values: &[(&TextField, &str)]) -> Option<MessageRef> {
    values
        .iter()
        .find(|(_, value)| value.is_empty())
        .map(|(field, _)| messages::field_required(field.key))
}

pub(crate) struct GuestStarContext {
    pub self_id: String,
    pub guest_id: String,
}

impl GuestStarContext {
    pub(crate) async fn resolve(
        transport: &dyn HelixTransport,
        identity: &SelfIdentity,
        target_login: &str,
    ) -> Result<Self, HelixError> {
        let self_id = identity.user_id().await?;
        let guest_id = resolve_user_id(transport, target_login).await?;
        Ok(Self { self_id, guest_id })
    }
}
