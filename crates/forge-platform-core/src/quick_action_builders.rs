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

    pub fn locked_because(self, reason: Option<UserText>) -> Self {
        Self {
            locked_reason: reason,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn label() -> MessageCode {
        MessageCode::from_static("test", "label")
    }

    fn action_with(fields: Vec<QuickActionField>) -> QuickAction {
        QuickAction::new(label(), "bolt", QuickActionAccent::default(), "test.step")
            .with_fields(fields)
    }

    #[test]
    fn a_new_action_cannot_run_until_it_is_enabled() {
        let action = action_with(Vec::new());

        assert!(!action.is_runnable());
        assert!(action.enabled_if(true).is_runnable());
    }

    #[test]
    fn built_defaults_reach_the_step_under_their_keys_and_kinds() {
        let action = action_with(vec![
            QuickActionField::toggle("flag", label(), true),
            QuickActionField::int("count", label(), 7, 1, 10),
            QuickActionField::static_choice(
                "mode",
                label(),
                "b",
                &[("a", label()), ("b", label())],
            ),
            QuickActionField::text("free", label()).with_default_text("hello"),
        ]);

        let step = action.merge_config(&BTreeMap::new());

        assert_eq!(
            step.config,
            BTreeMap::from([
                ("flag".to_owned(), Variant::Bool(true)),
                ("count".to_owned(), Variant::Int(7)),
                ("mode".to_owned(), Variant::String("b".to_owned())),
                ("free".to_owned(), Variant::String("hello".to_owned())),
            ])
        );
    }

    #[test]
    fn an_int_field_takes_its_bounds_in_low_then_high_order() {
        let field = QuickActionField::int("count", label(), 5, 2, 9);

        let verdicts = [1, 2, 9, 10].map(|n| field.kind.accepts(&QuickActionFieldValue::Int(n)));

        assert_eq!(verdicts, [false, true, true, false]);
    }

    #[test]
    fn a_static_choice_accepts_only_its_listed_values() {
        let field = QuickActionField::static_choice("mode", label(), "a", &[("a", label())]);

        let verdicts = ["a", "z"].map(|value| {
            field
                .kind
                .accepts(&QuickActionFieldValue::Text(value.to_owned()))
        });

        assert_eq!(verdicts, [true, false]);
    }

    #[test]
    fn with_config_seeds_the_step_and_a_field_value_overrides_it() {
        let action = action_with(vec![QuickActionField::text("free", label())]).with_config(
            BTreeMap::from([
                ("free".to_owned(), Variant::String("seed".to_owned())),
                ("fixed".to_owned(), Variant::Int(1)),
            ]),
        );
        let values = BTreeMap::from([(
            "free".to_owned(),
            QuickActionFieldValue::Text("typed".to_owned()),
        )]);

        let step = action.merge_config(&values);

        assert_eq!(
            step.config,
            BTreeMap::from([
                ("free".to_owned(), Variant::String("typed".to_owned())),
                ("fixed".to_owned(), Variant::Int(1)),
            ])
        );
    }
}
