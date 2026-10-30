use std::collections::BTreeMap;

use forge_platform_core::{
    OptionListId, QuickAction, QuickActionAccent, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue, QuickActionLiveness, QuickActions, SectionIcon,
};
use forge_types::SubActionStep;

use crate::client::ObsClient;
use crate::option_source::SCENE_FIELD;
use crate::vocabulary::option_lists::{
    INPUT_LIST, PROFILE_LIST, SCENE_COLLECTION_LIST, SCENE_LIST, SOURCE_LIST, TRANSITION_LIST,
};
use crate::vocabulary::{config_keys, sub_action_ids};

fn group_badge(group: &str) -> (SectionIcon, QuickActionAccent) {
    match group {
        "Scenes" => (SectionIcon::new("layout-grid"), QuickActionAccent::Brand),
        "Sources & audio" => (SectionIcon::new("stack-2"), QuickActionAccent::Info),
        "Stream & record" => (SectionIcon::new("broadcast"), QuickActionAccent::Danger),
        "Replay & capture" => (SectionIcon::new("device-floppy"), QuickActionAccent::Bits),
        "Profiles" => (SectionIcon::new("settings"), QuickActionAccent::Success),
        _ => (SectionIcon::new("dot"), QuickActionAccent::Brand),
    }
}

fn text_field(key: &str, label: &str, default: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Text,
        default: Some(QuickActionFieldValue::Text(default.to_owned())),
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn text_field_placeholder(
    key: &str,
    label: &str,
    default: &str,
    placeholder: &str,
) -> QuickActionField {
    QuickActionField {
        placeholder: Some(placeholder.to_owned()),
        ..text_field(key, label, default)
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
        required: false,
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
            config: BTreeMap::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        fields,
        collection: None,
    }
}

impl QuickActions for ObsClient {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.connection_state().is_connected();

        vec![
            quick_action(
                "Switch scene",
                "arrows-shuffle",
                QuickActionAccent::Brand,
                connected,
                "Scenes",
                false,
                sub_action_ids::SCENE_SWITCH_CURRENT,
                vec![choice_field(SCENE_FIELD, "Scene", SCENE_LIST).required()],
            ),
            quick_action(
                "Set preview scene",
                "layout",
                QuickActionAccent::Info,
                connected,
                "Scenes",
                false,
                sub_action_ids::SCENE_SET_PREVIEW,
                vec![choice_field(SCENE_FIELD, "Scene", SCENE_LIST).required()],
            ),
            quick_action(
                "Trigger transition",
                "transition-right",
                QuickActionAccent::Warning,
                connected,
                "Scenes",
                false,
                sub_action_ids::SCENE_SET_TRANSITION,
                vec![
                    choice_field(config_keys::TRANSITION, "Transition", TRANSITION_LIST).required(),
                ],
            ),
            quick_action(
                "Toggle studio mode",
                "aspect-ratio",
                QuickActionAccent::Success,
                connected,
                "Scenes",
                false,
                sub_action_ids::STUDIO_SET_ENABLED,
                vec![toggle_field(config_keys::ON, "Enabled", true)],
            ),
            quick_action(
                "Toggle source",
                "eye",
                QuickActionAccent::Success,
                connected,
                "Sources & audio",
                false,
                sub_action_ids::SOURCE_SET_VISIBLE,
                vec![
                    choice_field(SCENE_FIELD, "Scene", SCENE_LIST).required(),
                    choice_field(config_keys::SOURCE, "Source", SOURCE_LIST).required(),
                    toggle_field(config_keys::VISIBLE, "Visible", true),
                ],
            ),
            quick_action(
                "Set mute",
                "volume",
                QuickActionAccent::Info,
                connected,
                "Sources & audio",
                false,
                sub_action_ids::AUDIO_SET_MUTE,
                vec![
                    choice_field(config_keys::SOURCE, "Audio source", INPUT_LIST).required(),
                    toggle_field(config_keys::MUTED, "Muted", false),
                ],
            ),
            quick_action(
                "Set volume",
                "adjustments",
                QuickActionAccent::Brand,
                connected,
                "Sources & audio",
                false,
                sub_action_ids::AUDIO_SET_VOLUME,
                vec![
                    choice_field(config_keys::SOURCE, "Source", INPUT_LIST).required(),
                    text_field_placeholder(config_keys::VOLUME_DB, "Gain (dB)", "-18", "e.g. -6.0")
                        .required(),
                ],
            ),
            quick_action(
                "Apply filter",
                "filter",
                QuickActionAccent::Danger,
                connected,
                "Sources & audio",
                false,
                sub_action_ids::FILTER_SET_ENABLED,
                vec![
                    choice_field(config_keys::SOURCE, "Source", SOURCE_LIST).required(),
                    text_field(config_keys::FILTER, "Filter", "Chroma Key").required(),
                    toggle_field(config_keys::ENABLED, "Enabled", true),
                ],
            ),
            quick_action(
                "Refresh browser source",
                "refresh",
                QuickActionAccent::Warning,
                connected,
                "Sources & audio",
                false,
                sub_action_ids::BROWSER_REFRESH,
                vec![choice_field(config_keys::SOURCE, "Source", SOURCE_LIST).required()],
            ),
            quick_action(
                "Restart media",
                "player-track-next",
                QuickActionAccent::Bits,
                connected,
                "Sources & audio",
                false,
                sub_action_ids::MEDIA_RESTART,
                vec![choice_field(config_keys::SOURCE, "Source", SOURCE_LIST).required()],
            ),
            quick_action(
                "Start / stop stream",
                "player-play",
                QuickActionAccent::Success,
                connected,
                "Stream & record",
                true,
                sub_action_ids::STREAMING_SET_ACTIVE,
                vec![toggle_field(config_keys::ON, "Streaming", true)],
            ),
            quick_action(
                "Start / stop recording",
                "circle-dot",
                QuickActionAccent::Danger,
                connected,
                "Stream & record",
                false,
                sub_action_ids::RECORDING_SET_ACTIVE,
                vec![toggle_field(config_keys::ON, "Recording", false)],
            ),
            quick_action(
                "Pause / resume recording",
                "player-pause",
                QuickActionAccent::Warning,
                connected,
                "Stream & record",
                false,
                sub_action_ids::RECORDING_TOGGLE_PAUSE,
                Vec::new(),
            ),
            quick_action(
                "Toggle virtual camera",
                "device-tv",
                QuickActionAccent::Info,
                connected,
                "Stream & record",
                false,
                sub_action_ids::VIRTUALCAM_SET_ACTIVE,
                vec![toggle_field(config_keys::ON, "Enabled", false)],
            ),
            quick_action(
                "Toggle replay buffer",
                "player-record",
                QuickActionAccent::Brand,
                connected,
                "Replay & capture",
                false,
                sub_action_ids::REPLAY_SET_ACTIVE,
                vec![toggle_field(config_keys::ON, "Enabled", true)],
            ),
            quick_action(
                "Save replay buffer",
                "device-floppy",
                QuickActionAccent::Success,
                connected,
                "Replay & capture",
                false,
                sub_action_ids::REPLAY_SAVE,
                Vec::new(),
            ),
            quick_action(
                "Screenshot source",
                "camera",
                QuickActionAccent::Info,
                connected,
                "Replay & capture",
                false,
                sub_action_ids::CAPTURE_SCREENSHOT,
                vec![
                    choice_field(config_keys::SOURCE, "Source", SOURCE_LIST).required(),
                    text_field_placeholder(
                        config_keys::PATH,
                        "Save To",
                        "",
                        "D:/Pictures/screenshot.png",
                    )
                    .required(),
                ],
            ),
            quick_action(
                "Set recording folder",
                "folder",
                QuickActionAccent::Warning,
                connected,
                "Replay & capture",
                false,
                sub_action_ids::RECORDING_SET_DIRECTORY,
                vec![
                    text_field_placeholder(config_keys::PATH, "Folder", "", "D:/Recordings")
                        .required(),
                ],
            ),
            quick_action(
                "Switch profile",
                "user-cog",
                QuickActionAccent::Info,
                connected,
                "Profiles",
                false,
                sub_action_ids::PROFILE_SWITCH,
                vec![choice_field(config_keys::NAME, "Profile", PROFILE_LIST).required()],
            ),
            quick_action(
                "Switch scene collection",
                "layout-2",
                QuickActionAccent::Brand,
                connected,
                "Profiles",
                false,
                sub_action_ids::COLLECTION_SWITCH,
                vec![
                    choice_field(config_keys::NAME, "Collection", SCENE_COLLECTION_LIST).required(),
                ],
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use forge_platform_core::{BuiltinOptionSource, QuickActions};
    use forge_registry::SubActionRegistry;

    use super::*;
    use crate::client::ObsClient;
    use crate::runners::register_obs_sub_actions;
    use crate::sink::ObsSink;

    fn roster() -> Vec<QuickAction> {
        ObsClient::new_for_test("localhost:4455".to_owned()).actions()
    }

    fn registry() -> SubActionRegistry {
        let client = Arc::new(ObsClient::new_for_test("localhost:4455".to_owned()));
        let mut reg = SubActionRegistry::new();
        register_obs_sub_actions(&mut reg, client as Arc<dyn ObsSink>).unwrap();
        reg
    }

    #[test]
    fn all_actions_disabled_when_disconnected() {
        for action in &roster() {
            assert!(!action.enabled, "expected {} to be disabled", action.label);
        }
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
            let kind_id = &action.subaction_template.kind_id;
            let runner = reg
                .get(kind_id)
                .unwrap_or_else(|| panic!("{} targets unregistered {kind_id}", action.label));
            let step = action.merge_config(&supplied_values(action));
            let verdict = runner.validate_config(&step.config);
            assert!(
                verdict.is_ok(),
                "{} merges a config its runner rejects: {verdict:?}",
                action.label,
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
                    "{}: unchosen {:?} was not rejected",
                    action.label,
                    field.key,
                );
            }
        }
    }

    #[test]
    fn every_quick_action_config_and_field_key_is_one_its_runner_reads() {
        let reg = registry();
        for action in &roster() {
            let kind_id = &action.subaction_template.kind_id;
            let runner = reg.get(kind_id).unwrap();
            let known: BTreeSet<String> = runner.default_config().keys().cloned().collect();

            for key in action.subaction_template.config.keys() {
                assert!(
                    known.contains(key),
                    "{}: template key {key:?} is not read by {kind_id}",
                    action.label,
                );
            }
            for field in &action.fields {
                assert!(
                    known.contains(&field.key),
                    "{}: field key {:?} is not read by {kind_id}",
                    action.label,
                    field.key,
                );
            }
        }
    }

    #[test]
    fn only_the_stream_toggle_is_marked_destructive() {
        let destructive: Vec<String> = roster()
            .iter()
            .filter(|a| a.destructive)
            .map(|a| a.subaction_template.kind_id.clone())
            .collect();
        assert_eq!(destructive, vec!["obs.streaming.set_active".to_owned()]);
    }

    #[test]
    fn every_action_belongs_to_one_of_the_five_screen_groups() {
        let expected = BTreeSet::from([
            "Profiles",
            "Replay & capture",
            "Scenes",
            "Sources & audio",
            "Stream & record",
        ]);
        let actions = roster();
        let groups: BTreeSet<&str> = actions.iter().filter_map(|a| a.group.as_deref()).collect();
        assert_eq!(groups, expected);
        assert!(
            actions.iter().all(|a| a.group.is_some()),
            "every action must carry a group",
        );
    }

    #[test]
    fn every_group_resolves_to_its_own_badge_rather_than_the_fallback() {
        for action in &roster() {
            let group = action.group.as_deref().unwrap_or_default();
            let (icon, _) = group_badge(group);
            assert_ne!(
                icon.as_str(),
                "dot",
                "group {group:?} fell back to the default badge"
            );
        }
    }

    fn option_list_fields(action: &QuickAction) -> Vec<(String, String)> {
        action
            .fields
            .iter()
            .filter_map(|f| match &f.kind {
                QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(list)) => {
                    Some((f.key.clone(), list.as_str().to_owned()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn picker_fields_read_the_option_list_that_matches_the_runner_key() {
        let expected: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::from([
            ("obs.scene.switch_current", vec![("scene", SCENE_LIST)]),
            ("obs.scene.set_preview", vec![("scene", SCENE_LIST)]),
            (
                "obs.scene.set_transition",
                vec![("transition", TRANSITION_LIST)],
            ),
            (
                "obs.source.set_visible",
                vec![("scene", SCENE_LIST), ("source", SOURCE_LIST)],
            ),
            ("obs.audio.set_mute", vec![("source", INPUT_LIST)]),
            ("obs.audio.set_volume", vec![("source", INPUT_LIST)]),
            ("obs.filter.set_enabled", vec![("source", SOURCE_LIST)]),
            ("obs.browser.refresh", vec![("source", SOURCE_LIST)]),
            ("obs.media.restart", vec![("source", SOURCE_LIST)]),
            ("obs.capture.screenshot", vec![("source", SOURCE_LIST)]),
            ("obs.profile.switch", vec![("name", PROFILE_LIST)]),
            (
                "obs.collection.switch",
                vec![("name", SCENE_COLLECTION_LIST)],
            ),
        ]);
        let actual: BTreeMap<String, Vec<(String, String)>> = roster()
            .iter()
            .map(|a| (a.subaction_template.kind_id.clone(), option_list_fields(a)))
            .filter(|(_, fields)| !fields.is_empty())
            .collect();
        let expected: BTreeMap<String, Vec<(String, String)>> = expected
            .into_iter()
            .map(|(kind, fields)| {
                (
                    kind.to_owned(),
                    fields
                        .into_iter()
                        .map(|(k, l)| (k.to_owned(), l.to_owned()))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_option_list_a_field_reads_is_declared_by_the_option_source() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let declared: BTreeSet<String> = client
            .option_lists()
            .into_iter()
            .map(|m| m.id.as_str().to_owned())
            .collect();
        for action in &client.actions() {
            for (key, list) in option_list_fields(action) {
                assert!(
                    declared.contains(&list),
                    "{}: field {key:?} reads undeclared list {list:?}",
                    action.label,
                );
            }
        }
    }

    #[test]
    fn a_dependent_list_gets_its_dependency_as_a_field_whenever_the_runner_reads_that_key() {
        let registry = registry();
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let lists = client.option_lists();
        for action in &client.actions() {
            for (key, list) in option_list_fields(action) {
                let meta = lists.iter().find(|m| m.id.as_str() == list).unwrap();
                for dependency in &meta.depends_on {
                    let runner_reads_dependency = registry
                        .get(&action.subaction_template.kind_id)
                        .unwrap()
                        .config_fields()
                        .iter()
                        .any(|field| field.key() == dependency.as_str());
                    if !runner_reads_dependency {
                        continue;
                    }
                    assert!(
                        action.fields.iter().any(|f| &f.key == dependency),
                        "{}: {key:?} depends on {dependency:?} which the runner reads but no field sets",
                        action.label,
                    );
                }
            }
        }
    }
}
