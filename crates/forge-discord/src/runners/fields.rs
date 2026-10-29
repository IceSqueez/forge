use forge_registry::{OptionalField, TextEditor, TextField};

use crate::vocabulary::config_keys;

pub(super) const fn filled_text(
    key: &'static str,
    label: &'static str,
    editor: TextEditor,
) -> TextField {
    TextField {
        key,
        label,
        editor,
        default: "",
        required: true,
    }
}

pub(super) const fn optional_text(
    key: &'static str,
    label: &'static str,
    editor: TextEditor,
) -> TextField {
    TextField {
        key,
        label,
        editor,
        default: "",
        required: false,
    }
}

pub(super) const fn unlisted_text(
    key: &'static str,
    label: &'static str,
) -> OptionalField<TextField> {
    OptionalField::shared(optional_text(
        key,
        label,
        TextEditor::Line { placeholder: "" },
    ))
}

pub(super) fn present(text: String) -> Option<String> {
    (!text.trim().is_empty()).then_some(text)
}

pub(super) const WEBHOOK: TextField = filled_text(
    config_keys::WEBHOOK_NAME,
    "Webhook",
    TextEditor::Line {
        placeholder: "alerts",
    },
);

pub(super) const MESSAGE_ID: TextField = filled_text(
    config_keys::MESSAGE_ID,
    "Message ID",
    TextEditor::Line {
        placeholder: "%discord.message_id%",
    },
);
