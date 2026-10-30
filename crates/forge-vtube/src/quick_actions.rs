use std::collections::BTreeMap;

use forge_platform_core::{
    OptionListId, QuickAction, QuickActionAccent, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue, QuickActionGroup, QuickActionLiveness,
    QuickActions, SectionIcon,
};
use forge_types::{MessageCode, SubActionStep, Variant};

use crate::client::VTubeClient;
use crate::vocabulary::option_lists::{
    EXPRESSION_LIST, HOTKEY_LIST, ITEM_FILE_LIST, ITEM_INSTANCE_LIST, MODEL_LIST,
};
use crate::vocabulary::{config_keys, labels, sub_action_ids};

const EXPRESSION_FIELD: &str = config_keys::EXPRESSION_FILE;
const HOTKEY_FIELD: &str = config_keys::HOTKEY_ID;
const ITEM_FILE_FIELD: &str = config_keys::FILE_NAME;
const ITEM_INSTANCE_FIELD: &str = config_keys::ITEM_INSTANCE_ID;
const MODEL_FIELD: &str = config_keys::MODEL_ID;

fn config(pairs: impl IntoIterator<Item = (&'static str, Variant)>) -> BTreeMap<String, Variant> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

#[derive(Clone, Copy)]
enum Group {
    Expressions,
    HotkeysAnimation,
    Items,
    ModelScene,
}

impl Group {
    fn badge(self) -> QuickActionGroup {
        let (label, icon, accent) = match self {
            Self::Expressions => (
                labels::EXPRESSIONS,
                "mood-smile",
                QuickActionAccent::AccentPinkLight,
            ),
            Self::HotkeysAnimation => (
                labels::HOTKEYS_ANIMATION,
                "keyboard",
                QuickActionAccent::Brand,
            ),
            Self::Items => (labels::ITEMS, "confetti", QuickActionAccent::Info),
            Self::ModelScene => (labels::MODEL_SCENE, "user-square", QuickActionAccent::Bits),
        };
        QuickActionGroup {
            label: label.into(),
            icon: SectionIcon::new(icon),
            accent,
        }
    }
}

fn toggle_field(key: &str, label: MessageCode, default: bool) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Toggle,
        default: Some(QuickActionFieldValue::Toggle(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn choice_field(key: &str, label: MessageCode, list: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
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
    label: MessageCode,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    group: Group,
    destructive: bool,
    kind_id: &str,
    config: BTreeMap<String, Variant>,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    QuickAction {
        label: label.into(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason: None,
        liveness: QuickActionLiveness::Unknown,
        group: Some(group.badge()),
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
                labels::SET_EXPRESSION,
                "mood-smile",
                QuickActionAccent::AccentPinkLight,
                connected,
                Group::Expressions,
                false,
                sub_action_ids::EXPRESSION_SET,
                BTreeMap::new(),
                vec![choice_field(
                    EXPRESSION_FIELD,
                    labels::EXPRESSION,
                    EXPRESSION_LIST,
                )],
            ),
            quick_action(
                labels::TOGGLE_EXPRESSION,
                "toggle-left",
                QuickActionAccent::Warning,
                connected,
                Group::Expressions,
                false,
                sub_action_ids::EXPRESSION_SET,
                config([(config_keys::ACTIVE, Variant::Bool(false))]),
                vec![choice_field(
                    EXPRESSION_FIELD,
                    labels::EXPRESSION,
                    EXPRESSION_LIST,
                )],
            ),
            quick_action(
                labels::RESET_TO_IDLE,
                "refresh",
                QuickActionAccent::Info,
                connected,
                Group::Expressions,
                false,
                sub_action_ids::RESET_TO_IDLE,
                BTreeMap::new(),
                Vec::new(),
            ),
            quick_action(
                labels::TRIGGER_HOTKEY,
                "keyboard",
                QuickActionAccent::Brand,
                connected,
                Group::HotkeysAnimation,
                false,
                sub_action_ids::HOTKEY_TRIGGER,
                BTreeMap::new(),
                vec![choice_field(HOTKEY_FIELD, labels::HOTKEY, HOTKEY_LIST)],
            ),
            quick_action(
                labels::PLAY_ANIMATION,
                "run",
                QuickActionAccent::Success,
                connected,
                Group::HotkeysAnimation,
                false,
                sub_action_ids::HOTKEY_TRIGGER,
                BTreeMap::new(),
                vec![choice_field(HOTKEY_FIELD, labels::HOTKEY, HOTKEY_LIST)],
            ),
            quick_action(
                labels::THROW_ITEM,
                "photo",
                QuickActionAccent::Info,
                connected,
                Group::Items,
                false,
                sub_action_ids::ITEM_THROW,
                BTreeMap::new(),
                vec![choice_field(
                    ITEM_FILE_FIELD,
                    labels::ITEM_FILE,
                    ITEM_FILE_LIST,
                )],
            ),
            quick_action(
                labels::PIN_ITEM_TO_MODEL,
                "pin",
                QuickActionAccent::Warning,
                connected,
                Group::Items,
                false,
                sub_action_ids::ITEM_PIN,
                BTreeMap::new(),
                vec![choice_field(
                    ITEM_INSTANCE_FIELD,
                    labels::ITEM,
                    ITEM_INSTANCE_LIST,
                )],
            ),
            quick_action(
                labels::LOAD_ITEM,
                "plus",
                QuickActionAccent::Success,
                connected,
                Group::Items,
                false,
                sub_action_ids::ITEM_LOAD,
                BTreeMap::new(),
                vec![choice_field(
                    ITEM_FILE_FIELD,
                    labels::ITEM_FILE,
                    ITEM_FILE_LIST,
                )],
            ),
            quick_action(
                labels::REMOVE_ALL_ITEMS,
                "trash",
                QuickActionAccent::Danger,
                connected,
                Group::Items,
                true,
                sub_action_ids::ITEM_UNLOAD_ALL,
                BTreeMap::new(),
                Vec::new(),
            ),
            quick_action(
                labels::LOAD_MODEL,
                "user-square",
                QuickActionAccent::Bits,
                connected,
                Group::ModelScene,
                false,
                sub_action_ids::MODEL_LOAD,
                BTreeMap::new(),
                vec![choice_field(MODEL_FIELD, labels::MODEL, MODEL_LIST)],
            ),
            quick_action(
                labels::MOVE_SCALE_MODEL,
                "arrows-move",
                QuickActionAccent::Info,
                connected,
                Group::ModelScene,
                false,
                sub_action_ids::MODEL_MOVE,
                config([
                    (config_keys::X, Variant::Float(0.0)),
                    (config_keys::Y, Variant::Float(0.0)),
                    (config_keys::ROTATION, Variant::Float(0.0)),
                    (config_keys::SIZE, Variant::Float(0.0)),
                    (config_keys::DURATION, Variant::Float(0.3)),
                ]),
                Vec::new(),
            ),
            quick_action(
                labels::COLOR_TINT_OVERLAY,
                "color-swatch",
                QuickActionAccent::Warning,
                connected,
                Group::ModelScene,
                false,
                sub_action_ids::MODEL_TINT,
                config([
                    (config_keys::COLOR_R, Variant::Int(203)),
                    (config_keys::COLOR_G, Variant::Int(166)),
                    (config_keys::COLOR_B, Variant::Int(247)),
                ]),
                Vec::new(),
            ),
            quick_action(
                labels::TOGGLE_PHYSICS,
                "wind",
                QuickActionAccent::Success,
                connected,
                Group::ModelScene,
                false,
                sub_action_ids::MODEL_SET_PHYSICS,
                BTreeMap::new(),
                vec![toggle_field(config_keys::ENABLED, labels::ENABLED, true)],
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
    use forge_types::UserText;

    fn roster() -> Vec<QuickAction> {
        VTubeClient::new_for_test("ws://127.0.0.1:8001/").actions()
    }

    fn row(label: MessageCode) -> QuickAction {
        roster()
            .into_iter()
            .find(|a| a.label == UserText::from(label.clone()))
            .unwrap_or_else(|| panic!("roster is missing the '{label}' row"))
    }

    #[test]
    fn only_the_toggle_expression_row_presets_the_active_flag_and_presets_it_off() {
        let set = row(labels::SET_EXPRESSION);
        let toggle = row(labels::TOGGLE_EXPRESSION);

        assert_eq!(
            set.subaction_template.kind_id,
            toggle.subaction_template.kind_id
        );
        assert_eq!(set.subaction_template.config.get("active"), None);
        assert_eq!(
            toggle.subaction_template.config.get("active"),
            Some(&Variant::Bool(false))
        );
    }

    fn starts_blank(field: &QuickActionField) -> bool {
        match &field.default {
            None => true,
            Some(QuickActionFieldValue::Text(text)) => text.is_empty(),
            Some(_) => false,
        }
    }

    fn supplied_values(action: &QuickAction) -> BTreeMap<String, QuickActionFieldValue> {
        action
            .fields
            .iter()
            .filter(|field| starts_blank(field))
            .map(|field| {
                (
                    field.key.clone(),
                    QuickActionFieldValue::Text("supplied".to_owned()),
                )
            })
            .collect()
    }

    #[test]
    fn every_quick_action_merges_a_config_its_runner_accepts_once_blank_fields_are_filled() {
        let reg = registry();
        for action in &roster() {
            let runner = reg.get(&action.subaction_template.kind_id).unwrap();
            let step = action.merge_config(&supplied_values(action));
            let verdict = runner.validate_config(&step.config);
            assert!(
                verdict.is_ok(),
                "{} merges a config its runner rejects: {verdict:?}",
                action.label.code_or_text(),
            );
        }
    }

    #[test]
    fn every_required_field_left_blank_is_rejected_on_its_own_key() {
        let reg = registry();
        for action in &roster() {
            let runner = reg.get(&action.subaction_template.kind_id).unwrap();
            for field in action
                .fields
                .iter()
                .filter(|f| f.required && starts_blank(f))
            {
                let mut values = supplied_values(action);
                values.remove(&field.key);
                let rejected = runner
                    .validate_config(&action.merge_config(&values).config)
                    .err()
                    .and_then(|error| error.into_config_problem().field);
                assert_eq!(
                    rejected.as_deref(),
                    Some(field.key.as_str()),
                    "{}: blank {:?} was not rejected",
                    action.label.code_or_text(),
                    field.key,
                );
            }
        }
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
            (labels::SET_EXPRESSION, "expression_file", EXPRESSION_LIST),
            (
                labels::TOGGLE_EXPRESSION,
                "expression_file",
                EXPRESSION_LIST,
            ),
            (labels::TRIGGER_HOTKEY, "hotkey_id", HOTKEY_LIST),
            (labels::PLAY_ANIMATION, "hotkey_id", HOTKEY_LIST),
            (labels::THROW_ITEM, "file_name", ITEM_FILE_LIST),
            (
                labels::PIN_ITEM_TO_MODEL,
                "item_instance_id",
                ITEM_INSTANCE_LIST,
            ),
            (labels::LOAD_ITEM, "file_name", ITEM_FILE_LIST),
            (labels::LOAD_MODEL, "model_id", MODEL_LIST),
        ];
        let actual: BTreeMap<String, Vec<(String, String, bool)>> = roster()
            .iter()
            .map(|a| (a.label.code_or_text(), option_list_fields(a)))
            .filter(|(_, fields)| !fields.is_empty())
            .collect();
        let expected: BTreeMap<String, Vec<(String, String, bool)>> = expected
            .into_iter()
            .map(|(label, key, list)| {
                (
                    label.to_string(),
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
            let runner = reg.get(kind_id).unwrap_or_else(|| {
                panic!(
                    "{} targets unregistered {kind_id}",
                    action.label.code_or_text()
                )
            });
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
                    action.label.code_or_text(),
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
                    action.label.code_or_text(),
                );
            }
        }
    }

    #[test]
    fn only_the_irreversible_row_is_marked_destructive() {
        let destructive: BTreeSet<String> = roster()
            .into_iter()
            .filter(|a| a.destructive)
            .map(|a| a.label.code_or_text())
            .collect();

        assert_eq!(
            destructive,
            BTreeSet::from([labels::REMOVE_ALL_ITEMS.to_string()])
        );
    }

    #[test]
    fn all_actions_disabled_when_disconnected() {
        for action in roster() {
            assert!(
                !action.enabled,
                "{} must be disabled when disconnected",
                action.label.code_or_text()
            );
        }
    }

    #[test]
    fn every_code_the_client_shows_is_catalogued_in_both_languages() {
        use crate::emitted_codes::{emitted_codes, uncatalogued};
        use crate::vocabulary::messages::VTUBE_MESSAGES;
        use forge_platform_core::BuiltinContent;

        let client = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let emitted = emitted_codes(&serde_json::json!((client.actions(), client.sections())));

        assert!(!emitted.is_empty());
        assert_eq!(
            uncatalogued(&emitted, &[VTUBE_MESSAGES, forge_types::COMMON_MESSAGES]),
            Vec::<String>::new()
        );
    }
}
