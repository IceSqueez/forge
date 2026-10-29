use forge_platform_core::{SENDER_FIELD, SenderPreference};
use forge_registry::{
    BoundedTextField, ChoiceField, ConfigChoice, ConfigReason, FieldLiteral, FormField, TextEditor,
    TextField, TypedField, ValueBounds,
};
use forge_types::Variant;

use crate::account::user_target::TargetFields;
use crate::custom_rewards::is_valid_hex_color;
use crate::vocabulary::config_keys;

const UNRESOLVED_REFERENCE_MARKER: char = '%';
const UNRESOLVED_REFERENCE_DETAIL: &str = "has an unresolved variable reference";
const HEX_COLOR_DETAIL: &str = "expected a #RRGGBB color";
const USER_LOGIN_PLACEHOLDER: &str = "%user_login%";
pub(crate) const MAX_REASON_CHARS: i64 = 500;

pub(crate) const TARGET_LOGIN: TextField = TextField {
    key: config_keys::TARGET_USER_LOGIN,
    label: "Target Username",
    editor: TextEditor::Line {
        placeholder: USER_LOGIN_PLACEHOLDER,
    },
    default: "",
    required: true,
};

pub(crate) const MODERATION_TARGET: TargetFields = TargetFields {
    id: TextField {
        key: config_keys::TARGET_USER_ID,
        label: "Target User ID",
        editor: TextEditor::Line { placeholder: "" },
        default: "",
        required: false,
    },
    login: TextField {
        required: false,
        ..TARGET_LOGIN
    },
};

pub(crate) const OPTIONAL_REASON: BoundedTextField = BoundedTextField {
    key: config_keys::REASON,
    label: "Reason (optional, max 500 chars)",
    editor: TextEditor::Line { placeholder: "" },
    default: "",
    required: false,
    length: ValueBounds {
        min: None,
        max: Some(MAX_REASON_CHARS),
    },
};

pub(crate) const SENDER: ChoiceField<SendAs> = ChoiceField {
    key: SENDER_FIELD,
    label: "Send As",
    default: SendAs(SenderPreference::Default),
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SendAs(pub(crate) SenderPreference);

impl ConfigChoice for SendAs {
    const ALL: &'static [Self] = &[
        SendAs(SenderPreference::Default),
        SendAs(SenderPreference::Streamer),
        SendAs(SenderPreference::BotOnly),
    ];
    const WIRE_NAMES: &'static [&'static str] = SenderPreference::WIRE_NAMES;

    fn wire_name(self) -> &'static str {
        self.0.as_str()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModeToggle {
    Unchanged,
    On,
    Off,
}

impl ModeToggle {
    pub(crate) fn applied(self) -> Option<bool> {
        match self {
            Self::Unchanged => None,
            Self::On => Some(true),
            Self::Off => Some(false),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Unchanged => "Unchanged",
            Self::On => "On",
            Self::Off => "Off",
        }
    }
}

impl ConfigChoice for ModeToggle {
    const ALL: &'static [Self] = &[Self::Unchanged, Self::On, Self::Off];
    const WIRE_NAMES: &'static [&'static str] = &["unchanged", "on", "off"];

    fn wire_name(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::On => "on",
            Self::Off => "off",
        }
    }
}

pub(crate) const fn mode_toggle(key: &'static str, label: &'static str) -> ChoiceField<ModeToggle> {
    ChoiceField {
        key,
        label,
        default: ModeToggle::Unchanged,
    }
}

pub(crate) struct ResolvedIdField {
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) default: &'static str,
}

impl TypedField for ResolvedIdField {
    type Value = String;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::Text {
            key: self.key,
            label: self.label,
            placeholder: self.default,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(self.default.to_owned())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, String>, ConfigReason> {
        let Variant::String(text) = value else {
            return Err(ConfigReason::NotText);
        };
        match FieldLiteral::of_text(text) {
            Some(template) => Ok(template),
            None => self.parse_text(text).map(FieldLiteral::Value),
        }
    }

    fn parse_text(&self, text: &str) -> Result<String, ConfigReason> {
        if text.trim().is_empty() {
            Err(ConfigReason::Required)
        } else if text.contains(UNRESOLVED_REFERENCE_MARKER) {
            Err(ConfigReason::InvalidFormat(
                UNRESOLVED_REFERENCE_DETAIL.to_owned(),
            ))
        } else {
            Ok(text.to_owned())
        }
    }
}

pub(crate) struct HexColorField {
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) placeholder: &'static str,
}

impl TypedField for HexColorField {
    type Value = String;

    fn key(&self) -> &'static str {
        self.key
    }

    fn form_field(&self) -> FormField {
        FormField::Text {
            key: self.key,
            label: self.label,
            placeholder: self.placeholder,
        }
    }

    fn default_value(&self) -> Variant {
        Variant::String(String::new())
    }

    fn literal<'a>(&self, value: &'a Variant) -> Result<FieldLiteral<'a, String>, ConfigReason> {
        match value {
            Variant::String(text) => self.parse_text(text).map(FieldLiteral::Value),
            _ => Err(ConfigReason::NotText),
        }
    }

    fn parse_text(&self, text: &str) -> Result<String, ConfigReason> {
        if text.is_empty() || is_valid_hex_color(text) {
            Ok(text.to_owned())
        } else {
            Err(ConfigReason::InvalidFormat(HEX_COLOR_DETAIL.to_owned()))
        }
    }
}
