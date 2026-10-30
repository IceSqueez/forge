use std::collections::BTreeMap;

use forge_platform_core::{
    OptionListId, QuickAction, QuickActionAccent, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue, QuickActionGroup, QuickActionLiveness,
    QuickActions, SectionIcon,
};
use forge_types::{MessageCode, SubActionStep, UserText, VerbatimText};

use crate::client::ObsClient;
use crate::option_source::SCENE_FIELD;
use crate::vocabulary::option_lists::{
    INPUT_LIST, PROFILE_LIST, SCENE_COLLECTION_LIST, SCENE_LIST, SOURCE_LIST, TRANSITION_LIST,
};
use crate::vocabulary::{config_keys, labels, sub_action_ids};

#[derive(Clone, Copy)]
enum Group {
    Scenes,
    SourcesAudio,
    StreamRecord,
    ReplayCapture,
    Profiles,
}

impl Group {
    fn badge(self) -> QuickActionGroup {
        let (label, icon, accent) = match self {
            Self::Scenes => (labels::SCENES, "layout-grid", QuickActionAccent::Brand),
            Self::SourcesAudio => (labels::SOURCES_AUDIO, "stack-2", QuickActionAccent::Info),
            Self::StreamRecord => (
                labels::STREAM_RECORD,
                "broadcast",
                QuickActionAccent::Danger,
            ),
            Self::ReplayCapture => (
                labels::REPLAY_CAPTURE,
                "device-floppy",
                QuickActionAccent::Bits,
            ),
            Self::Profiles => (labels::PROFILES, "settings", QuickActionAccent::Success),
        };
        QuickActionGroup {
            label: label.into(),
            icon: SectionIcon::new(icon),
            accent,
        }
    }
}

fn text_field(key: &str, label: MessageCode, default: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.into(),
        kind: QuickActionFieldKind::Text,
        default: Some(QuickActionFieldValue::Text(default.to_owned())),
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn text_field_placeholder(
    key: &str,
    label: MessageCode,
    default: &str,
    placeholder: impl Into<UserText>,
) -> QuickActionField {
    QuickActionField {
        placeholder: Some(placeholder.into()),
        ..text_field(key, label, default)
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
        required: false,
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
                labels::SWITCH_SCENE,
                "arrows-shuffle",
                QuickActionAccent::Brand,
                connected,
                Group::Scenes,
                false,
                sub_action_ids::SCENE_SWITCH_CURRENT,
                vec![choice_field(SCENE_FIELD, labels::SCENE, SCENE_LIST).required()],
            ),
            quick_action(
                labels::SET_PREVIEW_SCENE,
                "layout",
                QuickActionAccent::Info,
                connected,
                Group::Scenes,
                false,
                sub_action_ids::SCENE_SET_PREVIEW,
                vec![choice_field(SCENE_FIELD, labels::SCENE, SCENE_LIST).required()],
            ),
            quick_action(
                labels::TRIGGER_TRANSITION,
                "transition-right",
                QuickActionAccent::Warning,
                connected,
                Group::Scenes,
                false,
                sub_action_ids::SCENE_SET_TRANSITION,
                vec![
                    choice_field(config_keys::TRANSITION, labels::TRANSITION, TRANSITION_LIST)
                        .required(),
                ],
            ),
            quick_action(
                labels::TOGGLE_STUDIO_MODE,
                "aspect-ratio",
                QuickActionAccent::Success,
                connected,
                Group::Scenes,
                false,
                sub_action_ids::STUDIO_SET_ENABLED,
                vec![toggle_field(config_keys::ON, labels::ENABLED, true)],
            ),
            quick_action(
                labels::TOGGLE_SOURCE,
                "eye",
                QuickActionAccent::Success,
                connected,
                Group::SourcesAudio,
                false,
                sub_action_ids::SOURCE_SET_VISIBLE,
                vec![
                    choice_field(SCENE_FIELD, labels::SCENE, SCENE_LIST).required(),
                    choice_field(config_keys::SOURCE, labels::SOURCE, SOURCE_LIST).required(),
                    toggle_field(config_keys::VISIBLE, labels::VISIBLE, true),
                ],
            ),
            quick_action(
                labels::SET_MUTE,
                "volume",
                QuickActionAccent::Info,
                connected,
                Group::SourcesAudio,
                false,
                sub_action_ids::AUDIO_SET_MUTE,
                vec![
                    choice_field(config_keys::SOURCE, labels::AUDIO_SOURCE, INPUT_LIST).required(),
                    toggle_field(config_keys::MUTED, labels::MUTED, false),
                ],
            ),
            quick_action(
                labels::SET_VOLUME,
                "adjustments",
                QuickActionAccent::Brand,
                connected,
                Group::SourcesAudio,
                false,
                sub_action_ids::AUDIO_SET_VOLUME,
                vec![
                    choice_field(config_keys::SOURCE, labels::SOURCE, INPUT_LIST).required(),
                    text_field_placeholder(
                        config_keys::VOLUME_DB,
                        labels::GAIN_DB,
                        "-18",
                        labels::VOLUME_EXAMPLE,
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::APPLY_FILTER,
                "filter",
                QuickActionAccent::Danger,
                connected,
                Group::SourcesAudio,
                false,
                sub_action_ids::FILTER_SET_ENABLED,
                vec![
                    choice_field(config_keys::SOURCE, labels::SOURCE, SOURCE_LIST).required(),
                    text_field(config_keys::FILTER, labels::FILTER, "Chroma Key").required(),
                    toggle_field(config_keys::ENABLED, labels::ENABLED, true),
                ],
            ),
            quick_action(
                labels::REFRESH_BROWSER_SOURCE,
                "refresh",
                QuickActionAccent::Warning,
                connected,
                Group::SourcesAudio,
                false,
                sub_action_ids::BROWSER_REFRESH,
                vec![choice_field(config_keys::SOURCE, labels::SOURCE, SOURCE_LIST).required()],
            ),
            quick_action(
                labels::RESTART_MEDIA,
                "player-track-next",
                QuickActionAccent::Bits,
                connected,
                Group::SourcesAudio,
                false,
                sub_action_ids::MEDIA_RESTART,
                vec![choice_field(config_keys::SOURCE, labels::SOURCE, SOURCE_LIST).required()],
            ),
            quick_action(
                labels::TOGGLE_STREAM,
                "player-play",
                QuickActionAccent::Success,
                connected,
                Group::StreamRecord,
                true,
                sub_action_ids::STREAMING_SET_ACTIVE,
                vec![toggle_field(config_keys::ON, labels::STREAMING, true)],
            ),
            quick_action(
                labels::TOGGLE_RECORDING,
                "circle-dot",
                QuickActionAccent::Danger,
                connected,
                Group::StreamRecord,
                false,
                sub_action_ids::RECORDING_SET_ACTIVE,
                vec![toggle_field(config_keys::ON, labels::RECORDING, false)],
            ),
            quick_action(
                labels::TOGGLE_RECORDING_PAUSE,
                "player-pause",
                QuickActionAccent::Warning,
                connected,
                Group::StreamRecord,
                false,
                sub_action_ids::RECORDING_TOGGLE_PAUSE,
                Vec::new(),
            ),
            quick_action(
                labels::TOGGLE_VIRTUAL_CAMERA,
                "device-tv",
                QuickActionAccent::Info,
                connected,
                Group::StreamRecord,
                false,
                sub_action_ids::VIRTUALCAM_SET_ACTIVE,
                vec![toggle_field(config_keys::ON, labels::ENABLED, false)],
            ),
            quick_action(
                labels::TOGGLE_REPLAY_BUFFER,
                "player-record",
                QuickActionAccent::Brand,
                connected,
                Group::ReplayCapture,
                false,
                sub_action_ids::REPLAY_SET_ACTIVE,
                vec![toggle_field(config_keys::ON, labels::ENABLED, true)],
            ),
            quick_action(
                labels::SAVE_REPLAY_BUFFER,
                "device-floppy",
                QuickActionAccent::Success,
                connected,
                Group::ReplayCapture,
                false,
                sub_action_ids::REPLAY_SAVE,
                Vec::new(),
            ),
            quick_action(
                labels::SCREENSHOT_SOURCE,
                "camera",
                QuickActionAccent::Info,
                connected,
                Group::ReplayCapture,
                false,
                sub_action_ids::CAPTURE_SCREENSHOT,
                vec![
                    choice_field(config_keys::SOURCE, labels::SOURCE, SOURCE_LIST).required(),
                    text_field_placeholder(
                        config_keys::PATH,
                        labels::SAVE_TO,
                        "",
                        VerbatimText::new("D:/Pictures/screenshot.png"),
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::SET_RECORDING_FOLDER,
                "folder",
                QuickActionAccent::Warning,
                connected,
                Group::ReplayCapture,
                false,
                sub_action_ids::RECORDING_SET_DIRECTORY,
                vec![
                    text_field_placeholder(
                        config_keys::PATH,
                        labels::FOLDER,
                        "",
                        VerbatimText::new("D:/Recordings"),
                    )
                    .required(),
                ],
            ),
            quick_action(
                labels::SWITCH_PROFILE,
                "user-cog",
                QuickActionAccent::Info,
                connected,
                Group::Profiles,
                false,
                sub_action_ids::PROFILE_SWITCH,
                vec![choice_field(config_keys::NAME, labels::PROFILE, PROFILE_LIST).required()],
            ),
            quick_action(
                labels::SWITCH_SCENE_COLLECTION,
                "layout-2",
                QuickActionAccent::Brand,
                connected,
                Group::Profiles,
                false,
                sub_action_ids::COLLECTION_SWITCH,
                vec![
                    choice_field(config_keys::NAME, labels::COLLECTION, SCENE_COLLECTION_LIST)
                        .required(),
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
            assert!(
                !action.enabled,
                "expected {} to be disabled",
                action.label.code_or_text()
            );
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
            let runner = reg.get(kind_id).unwrap_or_else(|| {
                panic!(
                    "{} targets unregistered {kind_id}",
                    action.label.code_or_text()
                )
            });
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
                    "{}: unchosen {:?} was not rejected",
                    action.label.code_or_text(),
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
                    action.label.code_or_text(),
                );
            }
            for field in &action.fields {
                assert!(
                    known.contains(&field.key),
                    "{}: field key {:?} is not read by {kind_id}",
                    action.label.code_or_text(),
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
            labels::PROFILES.to_string(),
            labels::REPLAY_CAPTURE.to_string(),
            labels::SCENES.to_string(),
            labels::SOURCES_AUDIO.to_string(),
            labels::STREAM_RECORD.to_string(),
        ]);
        let actions = roster();
        let groups: BTreeSet<String> = actions
            .iter()
            .filter_map(|a| a.group.as_ref())
            .map(|group| group.label.code_or_text())
            .collect();
        assert_eq!(groups, expected);
        assert!(
            actions.iter().all(|a| a.group.is_some()),
            "every action must carry a group",
        );
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
                    action.label.code_or_text(),
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
                        action.label.code_or_text(),
                    );
                }
            }
        }
    }
}
