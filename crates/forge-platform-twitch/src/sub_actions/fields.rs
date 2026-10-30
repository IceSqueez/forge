use forge_platform_core::{SENDER_FIELD, SenderPreference};
use forge_registry::{
    BoundedTextField, ChoiceField, ConfigChoice, ConfigReason, FieldLiteral, FormField, TextEditor,
    TextField, TypedField, ValueBounds,
};
use forge_types::Variant;

use crate::account::user_target::TargetFields;
use crate::custom_rewards::is_valid_hex_color;
use crate::vocabulary::{config_keys, messages};

const UNRESOLVED_REFERENCE_MARKER: char = '%';
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
            Err(ConfigReason::Coded(messages::unresolved_reference()))
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
            Err(ConfigReason::Coded(messages::invalid_hex_color()))
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_registry::{ConfigProblem, Interpolation};
    use forge_types::{ArgStack, SubActionConfig};

    use super::*;

    const PARENT: ResolvedIdField = ResolvedIdField {
        key: "parent",
        label: "Parent",
        default: "%message_id%",
    };

    const SWATCH: HexColorField = HexColorField {
        key: "swatch",
        label: "Swatch",
        placeholder: "",
    };

    fn stored(key: &str, value: Variant) -> SubActionConfig {
        SubActionConfig::from([(key.to_owned(), value)])
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn reason<T: std::fmt::Debug>(result: Result<T, ConfigProblem>) -> ConfigReason {
        result.unwrap_err().reason
    }

    #[test]
    fn resolved_id_reads_a_literal_id_verbatim() {
        assert_eq!(
            PARENT
                .read(&stored("parent", text("abc-123")), &ArgStack::new())
                .unwrap(),
            "abc-123"
        );
    }

    #[test]
    fn resolved_id_reads_the_interpolated_id_of_a_template() {
        let args = ArgStack::new().set("message_id".to_owned(), text("abc-123"));

        assert_eq!(
            PARENT.read(&SubActionConfig::new(), &args).unwrap(),
            "abc-123"
        );
    }

    #[test]
    fn resolved_id_refuses_a_blank_id_literal_or_interpolated() {
        let blank_args = ArgStack::new().set("message_id".to_owned(), text("  "));
        for (config, args) in [
            (stored("parent", text("")), ArgStack::new()),
            (stored("parent", text("   ")), ArgStack::new()),
            (SubActionConfig::new(), blank_args),
        ] {
            assert_eq!(reason(PARENT.read(&config, &args)), ConfigReason::Required);
        }
    }

    #[test]
    fn resolved_id_refuses_an_id_with_an_unresolved_reference() {
        let partial_args = ArgStack::new().set("message_id".to_owned(), text("abc"));
        for (config, args) in [
            (SubActionConfig::new(), ArgStack::new()),
            (stored("parent", text("%message_id%-%typo%")), partial_args),
            (stored("parent", text("abc%")), ArgStack::new()),
        ] {
            assert_eq!(
                reason(PARENT.read(&config, &args)),
                ConfigReason::Coded(messages::unresolved_reference())
            );
        }
    }

    #[test]
    fn resolved_id_refuses_a_non_text_value() {
        assert_eq!(
            reason(PARENT.read(&stored("parent", Variant::Int(7)), &ArgStack::new())),
            ConfigReason::NotText
        );
    }

    #[test]
    fn resolved_id_defers_a_template_until_run_time() {
        assert_eq!(
            PARENT.peek(&SubActionConfig::new(), Interpolation::Deferred),
            Ok(None)
        );
    }

    #[test]
    fn hex_color_admits_an_empty_value_or_a_six_digit_hex_color() {
        for literal in ["", "#9147FF", "#00aa00"] {
            assert_eq!(
                SWATCH
                    .read(&stored("swatch", text(literal)), &ArgStack::new())
                    .unwrap(),
                literal
            );
        }
    }

    #[test]
    fn hex_color_refuses_malformed_colors_and_templates() {
        for literal in ["#9147F", "9147FF0", "#9147FZ", "#9147FF0", "%color%"] {
            assert_eq!(
                reason(SWATCH.read(&stored("swatch", text(literal)), &ArgStack::new())),
                ConfigReason::Coded(messages::invalid_hex_color()),
                "{literal:?}"
            );
        }
    }

    #[test]
    fn hex_color_refuses_a_non_text_value() {
        assert_eq!(
            reason(SWATCH.read(&stored("swatch", Variant::Int(1)), &ArgStack::new())),
            ConfigReason::NotText
        );
    }

    #[test]
    fn sender_reads_absent_as_default_and_maps_each_wire_name() {
        for (raw, expected) in [
            (None, SenderPreference::Default),
            (Some("default"), SenderPreference::Default),
            (Some("streamer"), SenderPreference::Streamer),
            (Some("bot"), SenderPreference::BotOnly),
        ] {
            let config = raw
                .map(|raw| stored(SENDER_FIELD, text(raw)))
                .unwrap_or_default();
            assert_eq!(
                SENDER.read(&config, &ArgStack::new()).unwrap(),
                SendAs(expected),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn sender_refuses_unknown_or_non_text_values() {
        for rejected in [text("Bot"), text("bot_only"), text(""), Variant::Int(1)] {
            assert!(
                matches!(
                    reason(SENDER.read(&stored(SENDER_FIELD, rejected.clone()), &ArgStack::new())),
                    ConfigReason::InvalidChoice(_)
                ),
                "{rejected:?}"
            );
        }
    }
}
