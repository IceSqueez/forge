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
mod tests {
    use forge_platform_core::QuickActions;

    use crate::client::ObsClient;

    #[test]
    fn all_actions_disabled_when_disconnected() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let actions = client.actions();
        for action in &actions {
            assert!(!action.enabled, "expected {} to be disabled", action.label);
        }
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn start_recording_disabled_when_recording_active() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        {
            let mut snap = client.health_state.write().unwrap();
            snap.record_active = true;
        }
        let actions = client.actions();
        assert!(!actions[3].enabled);
    }
}
