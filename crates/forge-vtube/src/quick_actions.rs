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
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::BTreeSet;

    use forge_platform_core::{PickerKind, QuickAction, QuickActions};
    use forge_types::Variant;

    use crate::client::VTubeClient;

    fn roster() -> Vec<QuickAction> {
        VTubeClient::new_for_test("ws://127.0.0.1:8001/").actions()
    }

    fn row(label: &str) -> QuickAction {
        roster()
            .into_iter()
            .find(|a| a.label == label)
            .unwrap_or_else(|| panic!("roster is missing the '{label}' row"))
    }

    // Why: both rows drive the same runner, so the baked-in `active` preset is the only
    // thing that makes "Toggle expression" deactivate instead of duplicating "Set expression".
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

    // Why: an item FILE (something to spawn) and a loaded item INSTANCE (something already in
    // the scene) are separate id spaces in VTS; pinning against a file name silently no-ops.
    #[test]
    fn item_rows_pick_files_for_spawning_and_instances_for_pinning() {
        for label in ["Throw item", "Load item"] {
            assert_eq!(
                row(label).picker,
                Some(PickerKind::Item),
                "'{label}' spawns from an item file"
            );
        }
        assert_eq!(
            row("Pin item to model").picker,
            Some(PickerKind::ItemInstance)
        );
    }

    // Why: `destructive` is what puts a confirmation in front of the click.
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
