use std::collections::BTreeMap;

use forge_platform_core::{
    OptionListId, QuickAction, QuickActionAccent, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_types::{SubActionStep, Variant};

use crate::client::VTubeClient;
use crate::option_source::{
    EXPRESSION_LIST, HOTKEY_LIST, ITEM_FILE_LIST, ITEM_INSTANCE_LIST, MODEL_LIST,
};

const EXPRESSION_FIELD: &str = "expression_file";
const HOTKEY_FIELD: &str = "hotkey_id";
const ITEM_FILE_FIELD: &str = "file_name";
const ITEM_INSTANCE_FIELD: &str = "item_instance_id";
const MODEL_FIELD: &str = "model_id";

fn blank() -> Variant {
    Variant::String(String::new())
}

fn config(pairs: impl IntoIterator<Item = (&'static str, Variant)>) -> BTreeMap<String, Variant> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn group_badge(group: &str) -> (SectionIcon, QuickActionAccent) {
    match group {
        "Expressions" => (
            SectionIcon::new("mood-smile"),
            QuickActionAccent::AccentPinkLight,
        ),
        "Hotkeys & animation" => (SectionIcon::new("keyboard"), QuickActionAccent::Brand),
        "Items" => (SectionIcon::new("confetti"), QuickActionAccent::Info),
        "Model & scene" => (SectionIcon::new("user-square"), QuickActionAccent::Bits),
        _ => (SectionIcon::new("dot"), QuickActionAccent::Brand),
    }
}

fn toggle_field(key: &str, label: &str, default: bool) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Toggle,
        default: Some(QuickActionFieldValue::Toggle(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn choice_field(key: &str, label: &str, list: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(OptionListId::new(
            list,
        ))),
        default: None,
        placeholder: None,
        hint: None,
        required: true,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "each argument is one field of the quick-action template"
)]
fn quick_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    group: &str,
    destructive: bool,
    kind_id: &str,
    config: BTreeMap<String, Variant>,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    let (group_icon, group_accent) = group_badge(group);
    QuickAction {
        label: label.to_owned(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: Some(group.to_owned()),
        group_icon: Some(group_icon),
        group_accent: Some(group_accent),
        destructive,
        accent,
        subaction_template: SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        fields,
        collection: None,
    }
}

impl QuickActions for VTubeClient {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.connection_state().is_connected();

        vec![
            quick_action(
                "Set expression",
                "mood-smile",
                QuickActionAccent::AccentPinkLight,
                connected,
                "Expressions",
                false,
                "vtube.expression.set",
                config([(EXPRESSION_FIELD, blank()), ("active", Variant::Bool(true))]),
                vec![choice_field(
                    EXPRESSION_FIELD,
                    "Expression",
                    EXPRESSION_LIST,
                )],
            ),
            quick_action(
                "Toggle expression",
                "toggle-left",
                QuickActionAccent::Warning,
                connected,
                "Expressions",
                false,
                "vtube.expression.set",
                config([
                    (EXPRESSION_FIELD, blank()),
                    ("active", Variant::Bool(false)),
                ]),
                vec![choice_field(
                    EXPRESSION_FIELD,
                    "Expression",
                    EXPRESSION_LIST,
                )],
            ),
            quick_action(
                "Reset to idle",
                "refresh",
                QuickActionAccent::Info,
                connected,
                "Expressions",
                false,
                "vtube.params.reset",
                BTreeMap::new(),
                Vec::new(),
            ),
            quick_action(
                "Trigger hotkey",
                "keyboard",
                QuickActionAccent::Brand,
                connected,
                "Hotkeys & animation",
                false,
                "vtube.hotkey.trigger",
                config([(HOTKEY_FIELD, blank())]),
                vec![choice_field(HOTKEY_FIELD, "Hotkey", HOTKEY_LIST)],
            ),
            quick_action(
                "Play animation",
                "run",
                QuickActionAccent::Success,
                connected,
                "Hotkeys & animation",
                false,
                "vtube.hotkey.trigger",
                config([(HOTKEY_FIELD, blank())]),
                vec![choice_field(HOTKEY_FIELD, "Hotkey", HOTKEY_LIST)],
            ),
            quick_action(
                "Throw item",
                "photo",
                QuickActionAccent::Info,
                connected,
                "Items",
                false,
                "vtube.item.throw",
                config([(ITEM_FILE_FIELD, blank())]),
                vec![choice_field(ITEM_FILE_FIELD, "Item file", ITEM_FILE_LIST)],
            ),
            quick_action(
                "Pin item to model",
                "pin",
                QuickActionAccent::Warning,
                connected,
                "Items",
                false,
                "vtube.item.pin",
                config([(ITEM_INSTANCE_FIELD, blank()), ("pin", Variant::Bool(true))]),
                vec![choice_field(
                    ITEM_INSTANCE_FIELD,
                    "Item",
                    ITEM_INSTANCE_LIST,
                )],
            ),
            quick_action(
                "Load item",
                "plus",
                QuickActionAccent::Success,
                connected,
                "Items",
                false,
                "vtube.item.load",
                config([
                    (ITEM_FILE_FIELD, blank()),
                    ("unload_on_disconnect", Variant::Bool(true)),
                ]),
                vec![choice_field(ITEM_FILE_FIELD, "Item file", ITEM_FILE_LIST)],
            ),
            quick_action(
                "Remove all items",
                "trash",
                QuickActionAccent::Danger,
                connected,
                "Items",
                true,
                "vtube.item.unload_all",
                BTreeMap::new(),
                Vec::new(),
            ),
            quick_action(
                "Load model",
                "user-square",
                QuickActionAccent::Bits,
                connected,
                "Model & scene",
                false,
                "vtube.model.load",
                config([(MODEL_FIELD, blank())]),
                vec![choice_field(MODEL_FIELD, "Model", MODEL_LIST)],
            ),
            quick_action(
                "Move / scale model",
                "arrows-move",
                QuickActionAccent::Info,
                connected,
                "Model & scene",
                false,
                "vtube.model.move",
                config([
                    ("x", Variant::Float(0.0)),
                    ("y", Variant::Float(0.0)),
                    ("rotation", Variant::Float(0.0)),
                    ("size", Variant::Float(0.0)),
                    ("duration", Variant::Float(0.3)),
                ]),
                Vec::new(),
            ),
            quick_action(
                "Color tint overlay",
                "color-swatch",
                QuickActionAccent::Warning,
                connected,
                "Model & scene",
                false,
                "vtube.model.tint",
                config([
                    ("color_r", Variant::Int(203)),
                    ("color_g", Variant::Int(166)),
                    ("color_b", Variant::Int(247)),
                    ("color_a", Variant::Int(255)),
                ]),
                Vec::new(),
            ),
            quick_action(
                "Toggle physics",
                "wind",
                QuickActionAccent::Success,
                connected,
                "Model & scene",
                false,
                "vtube.model.set_physics",
                config([("enabled", Variant::Bool(true))]),
                vec![toggle_field("enabled", "Enabled", true)],
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use forge_platform_core::BuiltinOptionSource;
    use forge_registry::SubActionRegistry;

    use super::*;
    use crate::runners::register_vtube_sub_actions;
    use crate::runners::test_support::MockSink;
    use crate::sink::VTubeSink;

    fn roster() -> Vec<QuickAction> {
        VTubeClient::new_for_test("ws://127.0.0.1:8001/").actions()
    }

    fn row(label: &str) -> QuickAction {
        roster()
            .into_iter()
            .find(|a| a.label == label)
            .unwrap_or_else(|| panic!("roster is missing the '{label}' row"))
    }

    #[test]
    fn the_two_expression_rows_share_a_runner_but_preset_opposite_active_flags() {
        let set = row("Set expression");
        let toggle = row("Toggle expression");

        assert_eq!(
            set.subaction_template.kind_id,
            toggle.subaction_template.kind_id
        );
        assert_eq!(
            set.subaction_template.config.get("active"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(
            toggle.subaction_template.config.get("active"),
            Some(&Variant::Bool(false))
        );
    }

    fn option_list_fields(action: &QuickAction) -> Vec<(String, String, bool)> {
        action
            .fields
            .iter()
            .filter_map(|f| match &f.kind {
                QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(list)) => {
                    Some((f.key.clone(), list.as_str().to_owned(), f.required))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn picker_rows_read_the_option_list_that_matches_the_runner_key() {
        let expected = [
            ("Set expression", "expression_file", EXPRESSION_LIST),
            ("Toggle expression", "expression_file", EXPRESSION_LIST),
            ("Trigger hotkey", "hotkey_id", HOTKEY_LIST),
            ("Play animation", "hotkey_id", HOTKEY_LIST),
            ("Throw item", "file_name", ITEM_FILE_LIST),
            ("Pin item to model", "item_instance_id", ITEM_INSTANCE_LIST),
            ("Load item", "file_name", ITEM_FILE_LIST),
            ("Load model", "model_id", MODEL_LIST),
        ];
        let actual: BTreeMap<String, Vec<(String, String, bool)>> = roster()
            .iter()
            .map(|a| (a.label.clone(), option_list_fields(a)))
            .filter(|(_, fields)| !fields.is_empty())
            .collect();
        let expected: BTreeMap<String, Vec<(String, String, bool)>> = expected
            .into_iter()
            .map(|(label, key, list)| {
                (
                    label.to_owned(),
                    vec![(key.to_owned(), list.to_owned(), true)],
                )
            })
            .collect();
        assert_eq!(actual, expected);
    }

    fn registry() -> SubActionRegistry {
        let mut reg = SubActionRegistry::new();
        register_vtube_sub_actions(&mut reg, Arc::new(MockSink::new()) as Arc<dyn VTubeSink>)
            .unwrap();
        reg
    }

    #[test]
    fn every_quick_action_config_and_field_key_is_one_its_runner_reads() {
        let reg = registry();
        for action in &roster() {
            let kind_id = &action.subaction_template.kind_id;
            let runner = reg
                .get(kind_id)
                .unwrap_or_else(|| panic!("{} targets unregistered {kind_id}", action.label));
            let known: BTreeSet<String> = runner
                .default_config()
                .into_keys()
                .chain(runner.config_fields().iter().map(|f| f.key().to_owned()))
                .collect();
            let keys = action
                .subaction_template
                .config
                .keys()
                .chain(action.fields.iter().map(|f| &f.key));
            for key in keys {
                assert!(
                    known.contains(key),
                    "{}: key {key:?} is not read by {kind_id}",
                    action.label,
                );
            }
        }
    }

    #[test]
    fn every_blank_config_slot_has_a_field_that_fills_it() {
        for action in &roster() {
            for (key, value) in &action.subaction_template.config {
                if !matches!(value, Variant::String(s) if s.is_empty()) {
                    continue;
                }
                assert!(
                    action.fields.iter().any(|f| &f.key == key),
                    "{}: blank {key:?} has no field to fill it",
                    action.label,
                );
            }
        }
    }

    #[test]
    fn every_option_list_a_field_reads_is_declared_by_the_option_source() {
        let client = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let declared: BTreeSet<String> = client
            .option_lists()
            .into_iter()
            .map(|m| m.id.as_str().to_owned())
            .collect();
        for action in &client.actions() {
            for (key, list, _) in option_list_fields(action) {
                assert!(
                    declared.contains(&list),
                    "{}: field {key:?} reads undeclared list {list:?}",
                    action.label,
                );
            }
        }
    }

    #[test]
    fn only_the_irreversible_row_is_marked_destructive() {
        let destructive: BTreeSet<String> = roster()
            .into_iter()
            .filter(|a| a.destructive)
            .map(|a| a.label)
            .collect();

        assert_eq!(destructive, BTreeSet::from(["Remove all items".to_owned()]));
    }

    #[test]
    fn all_actions_disabled_when_disconnected() {
        for action in roster() {
            assert!(
                !action.enabled,
                "{} must be disabled when disconnected",
                action.label
            );
        }
    }
}
