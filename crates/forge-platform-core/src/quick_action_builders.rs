use std::collections::BTreeMap;

use forge_types::{MessageCode, SubActionStep, UserText, Variant};

use crate::builtin::{
    QuickAction, QuickActionAccent, QuickActionChoiceOption, QuickActionChoiceSource,
    QuickActionField, QuickActionFieldKind, QuickActionFieldValue, QuickActionGroup,
    QuickActionLiveness, SectionIcon,
};
use crate::option_source::OptionListId;

impl QuickActionField {
    fn of_kind(key: &str, label: MessageCode, kind: QuickActionFieldKind) -> Self {
        Self {
            key: key.to_owned(),
            label: label.into(),
            kind,
            default: None,
            placeholder: None,
            hint: None,
            required: false,
        }
    }

    pub fn text(key: &str, label: MessageCode) -> Self {
        Self::of_kind(key, label, QuickActionFieldKind::Text)
    }

    pub fn multiline(key: &str, label: MessageCode) -> Self {
        Self::of_kind(key, label, QuickActionFieldKind::Multiline)
    }

    pub fn toggle(key: &str, label: MessageCode, default: bool) -> Self {
        Self {
            default: Some(QuickActionFieldValue::Toggle(default)),
            ..Self::of_kind(key, label, QuickActionFieldKind::Toggle)
        }
    }

    pub fn int(key: &str, label: MessageCode, default: i64, min: i64, max: i64) -> Self {
        Self {
            default: Some(QuickActionFieldValue::Int(default)),
            ..Self::of_kind(key, label, QuickActionFieldKind::Int { min, max })
        }
    }

    pub fn option_list(key: &str, label: MessageCode, list: &str) -> Self {
        Self::of_kind(
            key,
            label,
            QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(OptionListId::new(
                list,
            ))),
        )
    }

    pub fn static_choice(
        key: &str,
        label: MessageCode,
        default: &str,
        options: &[(&str, MessageCode)],
    ) -> Self {
        let options = options
            .iter()
            .map(|(value, label)| QuickActionChoiceOption {
                value: (*value).to_owned(),
                label: label.clone().into(),
            })
            .collect();
        Self::of_kind(
            key,
            label,
            QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)),
        )
        .with_default_text(default)
    }

    pub fn with_default_text(self, default: &str) -> Self {
        Self {
            default: Some(QuickActionFieldValue::Text(default.to_owned())),
            ..self
        }
    }

    pub fn with_placeholder(self, placeholder: impl Into<UserText>) -> Self {
        Self {
            placeholder: Some(placeholder.into()),
            ..self
        }
    }

    pub fn with_hint(self, hint: impl Into<UserText>) -> Self {
        Self {
            hint: Some(hint.into()),
            ..self
        }
    }
}

impl QuickActionGroup {
    pub fn new(label: MessageCode, icon: &str, accent: QuickActionAccent) -> Self {
        Self {
            label: label.into(),
            icon: SectionIcon::new(icon),
            accent,
        }
    }
}

impl QuickAction {
    pub fn new(label: MessageCode, icon: &str, accent: QuickActionAccent, kind_id: &str) -> Self {
        Self::for_step(
            label,
            icon,
            accent,
            SubActionStep {
                kind_id: kind_id.to_owned(),
                config: BTreeMap::new(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            },
        )
    }

    pub fn for_step(
        label: MessageCode,
        icon: &str,
        accent: QuickActionAccent,
        subaction_template: SubActionStep,
    ) -> Self {
        Self {
            label: label.into(),
            icon: SectionIcon::new(icon),
            enabled: false,
            locked_reason: None,
            liveness: QuickActionLiveness::Unknown,
            group: None,
            destructive: false,
            accent,
            subaction_template,
            fields: Vec::new(),
            collection: None,
        }
    }

    pub fn enabled_if(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }

    pub fn in_group(self, group: QuickActionGroup) -> Self {
        Self {
            group: Some(group),
            ..self
        }
    }

    pub fn destructive(self) -> Self {
        self.destructive_if(true)
    }

    pub fn destructive_if(self, destructive: bool) -> Self {
        Self {
            destructive,
            ..self
        }
    }

    pub fn with_config(mut self, config: BTreeMap<String, Variant>) -> Self {
        self.subaction_template.config = config;
        self
    }

    pub fn with_fields(self, fields: Vec<QuickActionField>) -> Self {
        Self { fields, ..self }
    }
}
