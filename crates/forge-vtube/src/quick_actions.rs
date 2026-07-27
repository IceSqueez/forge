use std::collections::BTreeMap;

use forge_platform_core::{
    PickerKind, QuickAction, QuickActionAccent, QuickActionField, QuickActionFieldKind,
    QuickActionFieldValue, QuickActions, SectionIcon,
};
use forge_types::{SubActionStep, Variant};

use crate::client::VTubeClient;

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

impl QuickActions for VTubeClient {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.connection_state().is_connected();

        vec![
            // Expressions
            quick_action(
                "Set expression",
                "mood-smile",
                QuickActionAccent::AccentPinkLight,
                connected,
                "Expressions",
                false,
                "vtube.expression.set",
                config([
                    ("expression_file", blank()),
                    ("active", Variant::Bool(true)),
                ]),
                Some(PickerKind::Expression),
                Vec::new(),
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
                    ("expression_file", blank()),
                    ("active", Variant::Bool(false)),
                ]),
                Some(PickerKind::Expression),
                Vec::new(),
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
                None,
                Vec::new(),
            ),
            // Hotkeys & animation
            quick_action(
                "Trigger hotkey",
                "keyboard",
                QuickActionAccent::Brand,
                connected,
                "Hotkeys & animation",
                false,
                "vtube.hotkey.trigger",
                config([("hotkey_id", blank())]),
                Some(PickerKind::Hotkey),
                Vec::new(),
            ),
            quick_action(
                "Play animation",
                "run",
                QuickActionAccent::Success,
                connected,
                "Hotkeys & animation",
                false,
                "vtube.hotkey.trigger",
                config([("hotkey_id", blank())]),
                Some(PickerKind::Hotkey),
                Vec::new(),
            ),
            // Items
            quick_action(
                "Throw item",
                "photo",
                QuickActionAccent::Info,
                connected,
                "Items",
                false,
                "vtube.item.throw",
                config([("file_name", blank())]),
                Some(PickerKind::Item),
                Vec::new(),
            ),
            quick_action(
                "Pin item to model",
                "pin",
                QuickActionAccent::Warning,
                connected,
                "Items",
                false,
                "vtube.item.pin",
                config([("item_instance_id", blank()), ("pin", Variant::Bool(true))]),
                Some(PickerKind::ItemInstance),
                Vec::new(),
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
                    ("file_name", blank()),
                    ("unload_on_disconnect", Variant::Bool(true)),
                ]),
                Some(PickerKind::Item),
                Vec::new(),
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
                None,
                Vec::new(),
            ),
            // Model & scene
            quick_action(
                "Load model",
                "user-square",
                QuickActionAccent::Bits,
                connected,
                "Model & scene",
                false,
                "vtube.model.load",
                config([("model_id", blank())]),
                Some(PickerKind::Model),
                Vec::new(),
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
                None,
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
                None,
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
                None,
                vec![toggle_field("enabled", "Enabled", true)],
            ),
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
