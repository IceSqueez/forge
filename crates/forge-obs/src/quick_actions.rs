use std::collections::BTreeMap;

use forge_platform_core::{
    PickerKind, QuickAction, QuickActionAccent, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue, QuickActions, SectionIcon,
};
use forge_types::{SubActionStep, Variant};

use crate::client::ObsClient;

fn blank() -> Variant {
    Variant::String(String::new())
}

fn config(pairs: impl IntoIterator<Item = (&'static str, Variant)>) -> BTreeMap<String, Variant> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

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
    }
}

fn choice_field(key: &str, label: &str, picker: PickerKind) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Dynamic(picker)),
        default: None,
        placeholder: None,
        hint: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn quick_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    group: &str,
    destructive: bool,
    kind_id: &str,
    config: BTreeMap<String, Variant>,
    picker: Option<PickerKind>,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    let (group_icon, group_accent) = group_badge(group);
    QuickAction {
        label: label.to_owned(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason: None,
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
        picker,
        fields,
    }
}

impl QuickActions for ObsClient {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.connection_state().is_connected();
        let current_scene = self
            .catalog_state
            .try_read()
            .ok()
            .and_then(|c| c.current_scene.clone())
            .unwrap_or_default();

        vec![
            // Scenes
            quick_action(
                "Switch scene",
                "arrows-shuffle",
                QuickActionAccent::Brand,
                connected,
                "Scenes",
                false,
                "obs.scenes.switch_current",
                config([("scene", blank())]),
                Some(PickerKind::Scene),
                Vec::new(),
            ),
            quick_action(
                "Set preview scene",
                "layout",
                QuickActionAccent::Info,
                connected,
                "Scenes",
                false,
                "obs.scenes.set_preview",
                config([("scene", blank())]),
                Some(PickerKind::Scene),
                Vec::new(),
            ),
            quick_action(
                "Trigger transition",
                "transition-right",
                QuickActionAccent::Warning,
                connected,
                "Scenes",
                false,
                "obs.scenes.set_transition",
                config([("transition", blank())]),
                Some(PickerKind::Transition),
                Vec::new(),
            ),
            quick_action(
                "Toggle studio mode",
                "aspect-ratio",
                QuickActionAccent::Success,
                connected,
                "Scenes",
                false,
                "obs.studio.set_enabled",
                config([("on", Variant::Bool(true))]),
                None,
                vec![toggle_field("on", "Enabled", true)],
            ),
            // Sources & audio
            quick_action(
                "Toggle source",
                "eye",
                QuickActionAccent::Success,
                connected,
                "Sources & audio",
                false,
                "obs.sources.set_visible",
                config([
                    ("scene", Variant::String(current_scene.clone())),
                    ("source", blank()),
                    ("visible", Variant::Bool(true)),
                ]),
                Some(PickerKind::Source),
                vec![
                    choice_field("source", "Source", PickerKind::Source),
                    toggle_field("visible", "Visible", true),
                ],
            ),
            quick_action(
                "Set mute",
                "volume",
                QuickActionAccent::Info,
                connected,
                "Sources & audio",
                false,
                "obs.audio.set_mute",
                config([("source", blank()), ("muted", Variant::Bool(false))]),
                Some(PickerKind::Input),
                vec![
                    choice_field("source", "Audio source", PickerKind::Input),
                    toggle_field("muted", "Muted", false),
                ],
            ),
            quick_action(
                "Set volume",
                "adjustments",
                QuickActionAccent::Brand,
                connected,
                "Sources & audio",
                false,
                "obs.audio.set_volume",
                config([
                    ("source", blank()),
                    ("volume_db", Variant::String("-18".to_owned())),
                ]),
                Some(PickerKind::Input),
                vec![
                    choice_field("source", "Source", PickerKind::Input),
                    text_field_placeholder("volume_db", "Gain (dB)", "-18", "e.g. -6.0"),
                ],
            ),
            quick_action(
                "Apply filter",
                "filter",
                QuickActionAccent::Danger,
                connected,
                "Sources & audio",
                false,
                "obs.filter.set_enabled",
                config([
                    ("source", blank()),
                    ("filter", blank()),
                    ("enabled", Variant::Bool(true)),
                ]),
                Some(PickerKind::Source),
                vec![
                    choice_field("source", "Source", PickerKind::Source),
                    text_field("filter", "Filter", "Chroma Key"),
                    toggle_field("enabled", "Enabled", true),
                ],
            ),
            quick_action(
                "Refresh browser source",
                "refresh",
                QuickActionAccent::Warning,
                connected,
                "Sources & audio",
                false,
                "obs.browser.refresh",
                config([("source", blank())]),
                Some(PickerKind::Source),
                Vec::new(),
            ),
            quick_action(
                "Restart media",
                "player-track-next",
                QuickActionAccent::Bits,
                connected,
                "Sources & audio",
                false,
                "obs.media.restart",
                config([("source", blank())]),
                Some(PickerKind::Source),
                Vec::new(),
            ),
            // Stream & record
            quick_action(
                "Start / stop stream",
                "player-play",
                QuickActionAccent::Success,
                connected,
                "Stream & record",
                true,
                "obs.stream.set_active",
                config([("on", Variant::Bool(true))]),
                None,
                vec![toggle_field("on", "Streaming", true)],
            ),
            quick_action(
                "Start / stop recording",
                "circle-dot",
                QuickActionAccent::Danger,
                connected,
                "Stream & record",
                false,
                "obs.record.set_active",
                config([("on", Variant::Bool(false))]),
                None,
                vec![toggle_field("on", "Recording", false)],
            ),
            quick_action(
                "Pause / resume recording",
                "player-pause",
                QuickActionAccent::Warning,
                connected,
                "Stream & record",
                false,
                "obs.record.toggle_pause",
                BTreeMap::new(),
                None,
                Vec::new(),
            ),
            quick_action(
                "Toggle virtual camera",
                "device-tv",
                QuickActionAccent::Info,
                connected,
                "Stream & record",
                false,
                "obs.virtualcam.set_active",
                config([("on", Variant::Bool(false))]),
                None,
                vec![toggle_field("on", "Enabled", false)],
            ),
            // Replay & capture
            quick_action(
                "Toggle replay buffer",
                "player-record",
                QuickActionAccent::Brand,
                connected,
                "Replay & capture",
                false,
                "obs.replay.set_active",
                config([("on", Variant::Bool(true))]),
                None,
                vec![toggle_field("on", "Enabled", true)],
            ),
            quick_action(
                "Save replay buffer",
                "device-floppy",
                QuickActionAccent::Success,
                connected,
                "Replay & capture",
                false,
                "obs.replay.save",
                BTreeMap::new(),
                None,
                Vec::new(),
            ),
            quick_action(
                "Screenshot source",
                "camera",
                QuickActionAccent::Info,
                connected,
                "Replay & capture",
                false,
                "obs.capture.screenshot",
                config([("source", blank()), ("path", blank())]),
                Some(PickerKind::Source),
                vec![
                    choice_field("source", "Source", PickerKind::Source),
                    text_field_placeholder("path", "Save To", "", "~/Pictures/screenshot.png"),
                ],
            ),
            quick_action(
                "Set recording folder",
                "folder",
                QuickActionAccent::Warning,
                connected,
                "Replay & capture",
                false,
                "obs.record.set_directory",
                config([("path", Variant::String("~/Recordings".to_owned()))]),
                None,
                vec![text_field("path", "Folder", "~/Recordings")],
            ),
            // Profiles
            quick_action(
                "Switch profile",
                "user-cog",
                QuickActionAccent::Info,
                connected,
                "Profiles",
                false,
                "obs.profile.switch",
                config([("name", blank())]),
                Some(PickerKind::Profile),
                vec![choice_field("name", "Profile", PickerKind::Profile)],
            ),
            quick_action(
                "Switch scene collection",
                "layout-2",
                QuickActionAccent::Brand,
                connected,
                "Profiles",
                false,
                "obs.scene_collection.switch",
                config([("name", blank())]),
                Some(PickerKind::SceneCollection),
                vec![choice_field(
                    "name",
                    "Collection",
                    PickerKind::SceneCollection,
                )],
            ),
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use forge_platform_core::QuickActions;
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

    /// Blank string slots are the ones the picker or a form field fills in before the step runs,
    /// so validating the raw template would only re-assert that they are still blank.
    fn filled(config: &BTreeMap<String, Variant>) -> BTreeMap<String, Variant> {
        config
            .iter()
            .map(|(k, v)| match v {
                Variant::String(s) if s.is_empty() => {
                    (k.clone(), Variant::String("supplied".to_owned()))
                }
                other => (k.clone(), other.clone()),
            })
            .collect()
    }

    #[test]
    fn every_quick_action_targets_a_registered_runner_that_accepts_its_template() {
        let reg = registry();
        for action in &roster() {
            let kind_id = &action.subaction_template.kind_id;
            let runner = reg
                .get(kind_id)
                .unwrap_or_else(|| panic!("{} targets unregistered {kind_id}", action.label));
            assert!(
                runner
                    .validate_config(&filled(&action.subaction_template.config))
                    .is_ok(),
                "{} ships a template config its runner rejects",
                action.label,
            );
        }
    }

    // Why: a config key the runner never reads is silently dropped by effective_config, so the
    // button runs and reports success while doing nothing the user asked for.
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

    // Why: an action whose only blank config slot has neither a picker nor an editable field
    // renders as a button the user can press but cannot fill in.
    #[test]
    fn every_action_with_a_blank_config_slot_offers_a_picker_or_a_field() {
        for action in &roster() {
            let has_blank = action
                .subaction_template
                .config
                .values()
                .any(|v| matches!(v, Variant::String(s) if s.is_empty()));
            if !has_blank {
                continue;
            }
            assert!(
                action.picker.is_some() || !action.fields.is_empty(),
                "{} has a blank config slot with no way to fill it",
                action.label,
            );
        }
    }

    #[test]
    fn only_the_stream_toggle_is_marked_destructive() {
        let destructive: Vec<String> = roster()
            .iter()
            .filter(|a| a.destructive)
            .map(|a| a.subaction_template.kind_id.clone())
            .collect();
        assert_eq!(destructive, vec!["obs.stream.set_active".to_owned()]);
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

    // Why: group_badge falls through to a generic "dot" badge for an unknown group string, so a
    // typo in a group name degrades the header silently instead of failing.
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

    #[test]
    fn name_pickers_are_wired_to_the_actions_that_switch_by_name() {
        let by_kind: Vec<(String, Option<PickerKind>)> = roster()
            .iter()
            .map(|a| (a.subaction_template.kind_id.clone(), a.picker))
            .collect();
        for (kind_id, expected) in [
            ("obs.scenes.switch_current", PickerKind::Scene),
            ("obs.scenes.set_preview", PickerKind::Scene),
            ("obs.scenes.set_transition", PickerKind::Transition),
            ("obs.profile.switch", PickerKind::Profile),
            ("obs.scene_collection.switch", PickerKind::SceneCollection),
        ] {
            let found = by_kind
                .iter()
                .find(|(k, _)| k == kind_id)
                .unwrap_or_else(|| panic!("{kind_id} missing from the roster"));
            assert_eq!(found.1, Some(expected), "{kind_id} picker");
        }
    }
}
