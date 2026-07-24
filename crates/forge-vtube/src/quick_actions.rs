use std::collections::BTreeMap;

use forge_platform_core::{PickerKind, QuickAction, QuickActionAccent, QuickActions, SectionIcon};
use forge_types::{SubActionStep, Variant};

use crate::client::VTubeClient;

impl QuickActions for VTubeClient {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.connection_state().is_connected();

        vec![
            QuickAction {
                label: "Trigger Hotkey".to_owned(),
                icon: SectionIcon::new("bolt"),
                enabled: connected,
                locked_reason: None,
                group: None,
                destructive: false,
                accent: QuickActionAccent::Brand,
                subaction_template: SubActionStep {
                    kind_id: "vtube.hotkey.trigger".to_owned(),
                    config: BTreeMap::from([(
                        "hotkey_id".to_owned(),
                        Variant::String(String::new()),
                    )]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: Some(PickerKind::Hotkey),
            },
            QuickAction {
                label: "Activate Expression".to_owned(),
                icon: SectionIcon::new("mood-smile"),
                enabled: connected,
                locked_reason: None,
                group: None,
                destructive: false,
                accent: QuickActionAccent::Brand,
                subaction_template: SubActionStep {
                    kind_id: "vtube.expression.set".to_owned(),
                    config: BTreeMap::from([
                        ("expression_file".to_owned(), Variant::String(String::new())),
                        ("active".to_owned(), Variant::Bool(true)),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: Some(PickerKind::Expression),
            },
            QuickAction {
                label: "Load Model".to_owned(),
                icon: SectionIcon::new("user"),
                enabled: connected,
                locked_reason: None,
                group: None,
                destructive: false,
                accent: QuickActionAccent::Brand,
                subaction_template: SubActionStep {
                    kind_id: "vtube.model.load".to_owned(),
                    config: BTreeMap::from([(
                        "model_id".to_owned(),
                        Variant::String(String::new()),
                    )]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Move Model".to_owned(),
                icon: SectionIcon::new("arrows-move"),
                enabled: connected,
                locked_reason: None,
                group: None,
                destructive: false,
                accent: QuickActionAccent::Brand,
                subaction_template: SubActionStep {
                    kind_id: "vtube.model.move".to_owned(),
                    config: BTreeMap::from([
                        ("x".to_owned(), Variant::Float(0.0)),
                        ("y".to_owned(), Variant::Float(0.0)),
                        ("rotation".to_owned(), Variant::Float(0.0)),
                        ("time_in_seconds".to_owned(), Variant::Float(1.0)),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_platform_core::{PickerKind, QuickActions};
    use forge_types::Variant;

    use crate::client::VTubeClient;

    #[test]
    fn trigger_hotkey_has_correct_kind_id_and_hotkey_picker() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let actions = c.actions();
        let hk = &actions[0];
        assert_eq!(hk.label, "Trigger Hotkey");
        assert_eq!(hk.subaction_template.kind_id, "vtube.hotkey.trigger");
        assert!(hk.subaction_template.config.contains_key("hotkey_id"));
        assert_eq!(hk.picker, Some(PickerKind::Hotkey));
    }

    #[test]
    fn load_model_has_no_picker_and_model_id_in_config() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let actions = c.actions();
        let lm = &actions[2];
        assert_eq!(lm.label, "Load Model");
        assert_eq!(lm.subaction_template.kind_id, "vtube.model.load");
        assert!(lm.subaction_template.config.contains_key("model_id"));
        assert!(lm.picker.is_none());
    }

    #[test]
    fn move_model_config_has_all_four_fields() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let actions = c.actions();
        let mv = &actions[3];
        assert_eq!(mv.subaction_template.kind_id, "vtube.model.move");
        let cfg = &mv.subaction_template.config;
        assert!(matches!(cfg.get("x"), Some(Variant::Float(_))));
        assert!(matches!(cfg.get("y"), Some(Variant::Float(_))));
        assert!(matches!(cfg.get("rotation"), Some(Variant::Float(_))));
        assert!(matches!(
            cfg.get("time_in_seconds"),
            Some(Variant::Float(_))
        ));
    }

    #[test]
    fn all_actions_disabled_when_disconnected() {
        let c = VTubeClient::new_for_test("ws://127.0.0.1:8001/");
        let actions = c.actions();
        for action in &actions {
            assert!(
                !action.enabled,
                "{} must be disabled when disconnected",
                action.label
            );
        }
    }
}
