use forge_registry::{TextEditor, TextField};

use crate::vocabulary::config_keys;

pub(super) const CHANNEL_ID: TextField = TextField {
    key: config_keys::CHANNEL_ID,
    label: "Channel ID",
    editor: TextEditor::Line {
        placeholder: "UC... or %user_id%",
    },
    default: "",
    required: true,
};
