use std::collections::BTreeMap;

use forge_platform_core::{ConnectionState, PickerKind, QuickAction, QuickActions, SectionIcon};
use forge_types::{SubActionStep, Variant};

use crate::client::ObsClient;

impl QuickActions for ObsClient {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.connection_state() == ConnectionState::Connected;
        let recording = self
            .health_state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .record_active;

        vec![
            QuickAction {
                label: "Switch Scene".to_owned(),
                icon: SectionIcon::new("arrows-shuffle"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "obs.scenes.switch_current".to_owned(),
                    config: BTreeMap::from([("scene".to_owned(), Variant::String(String::new()))]),
                    enabled: true,
                    label: None,
                },
                picker: Some(PickerKind::Scene),
            },
            QuickAction {
                label: "Toggle Source".to_owned(),
                icon: SectionIcon::new("eye"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "obs.sources.set_visible".to_owned(),
                    config: BTreeMap::from([
                        ("scene".to_owned(), Variant::String(String::new())),
                        ("source".to_owned(), Variant::String(String::new())),
                        ("visible".to_owned(), Variant::Bool(true)),
                    ]),
                    enabled: true,
                    label: None,
                },
                picker: Some(PickerKind::Source),
            },
            QuickAction {
                label: "Set Mute".to_owned(),
                icon: SectionIcon::new("volume"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "obs.audio.set_mute".to_owned(),
                    config: BTreeMap::from([
                        ("source".to_owned(), Variant::String(String::new())),
                        ("muted".to_owned(), Variant::Bool(true)),
                    ]),
                    enabled: true,
                    label: None,
                },
                picker: Some(PickerKind::Input),
            },
            QuickAction {
                label: "Start Recording".to_owned(),
                icon: SectionIcon::new("record"),
                enabled: connected && !recording,
                subaction_template: SubActionStep {
                    kind_id: "obs.record.start".to_owned(),
                    config: BTreeMap::new(),
                    enabled: true,
                    label: None,
                },
                picker: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use forge_platform_core::QuickActions;

    use crate::client::ObsClient;

    #[test]
    fn actions_returns_exactly_four() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let qa: &dyn QuickActions = &client;
        assert_eq!(qa.actions().len(), 4);
    }

    #[test]
    fn actions_labels_in_order() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let actions = client.actions();
        assert_eq!(actions[0].label, "Switch Scene");
        assert_eq!(actions[1].label, "Toggle Source");
        assert_eq!(actions[2].label, "Set Mute");
        assert_eq!(actions[3].label, "Start Recording");
    }

    #[test]
    fn actions_kind_ids_are_correct() {
        let client = ObsClient::new_for_test("localhost:4455".to_owned());
        let actions = client.actions();
        assert_eq!(
            actions[0].subaction_template.kind_id,
            "obs.scenes.switch_current"
        );
        assert_eq!(
            actions[1].subaction_template.kind_id,
            "obs.sources.set_visible"
        );
        assert_eq!(actions[2].subaction_template.kind_id, "obs.audio.set_mute");
        assert_eq!(actions[3].subaction_template.kind_id, "obs.record.start");
    }

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
