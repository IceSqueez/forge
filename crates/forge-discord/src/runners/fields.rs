use forge_registry::{
    ConfigReason, FieldLiteral, FormField, OptionalField, TextEditor, TextField, TypedField,
};
use forge_types::Variant;

use crate::vocabulary::config_keys;

pub(super) struct FilledText(TextField);

impl TypedField for FilledText {
    type Value = String;

    fn key(&self) -> &'static str {
        self.0.key
    }

    fn form_field(&self) -> FormField {
        self.0.form_field()
    }

    fn default_value(&self) -> Variant {
        self.0.default_value()
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, String>, ConfigReason> {
        self.0.literal(value)
    }

    fn parse_text(&self, text: &str) -> Result<String, ConfigReason> {
        if text.trim().is_empty() {
            Err(ConfigReason::Required)
        } else {
            self.0.parse_text(text)
        }
    }
}

pub(super) const fn filled_text(
    key: &'static str,
    label: &'static str,
    editor: TextEditor,
) -> FilledText {
    FilledText(TextField {
        key,
        label,
        editor,
        default: "",
        required: true,
    })
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
    OptionalField {
        field: optional_text(key, label, TextEditor::Line { placeholder: "" }),
    }
}

pub(super) fn present(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

pub(super) const WEBHOOK: FilledText = filled_text(
    config_keys::WEBHOOK_NAME,
    "Webhook",
    TextEditor::Line {
        placeholder: "alerts",
    },
);

pub(super) const MESSAGE_ID: FilledText = filled_text(
    config_keys::MESSAGE_ID,
    "Message ID",
    TextEditor::Line {
        placeholder: "%discord.message_id%",
    },
);
