use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use super::{combo_condition, config_accepts_combo};
use crate::bindings::row::HotkeyEdge;
use crate::vocabulary::{event_kinds, fields, trigger_ids, variables};

pub(crate) struct HotkeyEdgeDescriptor {
    edge: HotkeyEdge,
}

impl HotkeyEdgeDescriptor {
    pub(crate) fn new(edge: HotkeyEdge) -> Self {
        Self { edge }
    }
}

fn integer_variable(stack: ArgStack, event: &Event, field: &str, name: &str) -> ArgStack {
    match event.payload.get(field).and_then(|v| v.as_u64()) {
        Some(value) => stack.set(
            name.to_owned(),
            Variant::Int(i64::try_from(value).unwrap_or(i64::MAX)),
        ),
        None => stack,
    }
}

fn declared(name: &str, kind: VariantKind, label: &str) -> DeclaredVariable {
    DeclaredVariable {
        name: name.to_owned(),
        kind,
        label: label.to_owned(),
        synthesis: None,
    }
}

impl TriggerKindDescriptor for HotkeyEdgeDescriptor {
    fn id(&self) -> &str {
        match self.edge {
            HotkeyEdge::Press => trigger_ids::GLOBAL_PRESSED,
            HotkeyEdge::Release => trigger_ids::GLOBAL_RELEASED,
        }
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Integration
    }

    fn label(&self) -> &str {
        match self.edge {
            HotkeyEdge::Press => "Hotkey Pressed",
            HotkeyEdge::Release => "Hotkey Released",
        }
    }

    fn summary(&self) -> &str {
        match self.edge {
            HotkeyEdge::Press => "Fires when a registered global hotkey combo is pressed.",
            HotkeyEdge::Release => "Fires when a held global hotkey combo is released.",
        }
    }

    fn search_text(&self) -> &str {
        match self.edge {
            HotkeyEdge::Press => "hotkey global shortcut combo keyboard pressed",
            HotkeyEdge::Release => {
                "hotkey global shortcut combo keyboard released hold push to talk"
            }
        }
    }

    fn icon_name(&self) -> &str {
        "keyboard"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: crate::COMBO_FIELD,
            label: "Hotkey combo",
            placeholder: "e.g. Ctrl+Shift+1",
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> UserText {
        combo_condition(config)
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Hotkey),
            kind_prefix: Some(event_kinds::PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != self.edge.kind_id() {
            return false;
        }
        if event.source != EventSource::Hotkey {
            return false;
        }
        config_accepts_combo(config, event)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(combo) = event.payload.get(fields::COMBO).and_then(|v| v.as_str()) {
            stack = stack.set(
                variables::COMBO.to_owned(),
                Variant::String(combo.to_owned()),
            );
        }
        stack = integer_variable(stack, event, fields::ID, variables::ID);
        stack = integer_variable(stack, event, fields::TIMESTAMP_US, variables::TIMESTAMP_US);
        if self.edge == HotkeyEdge::Release {
            stack = integer_variable(stack, event, fields::HOLD_MS, variables::HOLD_MS);
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        let mut variables = vec![
            declared(variables::COMBO, VariantKind::String, "Hotkey combo"),
            declared(variables::ID, VariantKind::Int, "Hotkey ID"),
            declared(
                variables::TIMESTAMP_US,
                VariantKind::Int,
                "Timestamp (microseconds)",
            ),
        ];
        if self.edge == HotkeyEdge::Release {
            variables.push(declared(
                variables::HOLD_MS,
                VariantKind::Int,
                "Held for (milliseconds)",
            ));
        }
        Some(VariableSchema { variables })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::VerbatimText;
    use serde_json::json;

    use super::*;
    use crate::vocabulary::messages;

    fn event_of(edge: HotkeyEdge, combo: &str) -> Event {
        Event::new(
            EventSource::Hotkey,
            edge.kind_id(),
            json!({
                "combo": combo,
                "id": 1u32,
                "timestamp_us": 0u64,
                "hold_ms": 1250u64,
            }),
        )
    }

    fn combo_config(combo: &str) -> BTreeMap<String, Variant> {
        BTreeMap::from([("combo".to_owned(), Variant::String(combo.to_owned()))])
    }

    #[test]
    fn matches_trigger_admits_only_its_own_edge_whose_combo_the_config_accepts() {
        for edge in [HotkeyEdge::Press, HotkeyEdge::Release] {
            let descriptor = HotkeyEdgeDescriptor::new(edge);
            let cases = [
                (
                    "empty config takes any combo",
                    BTreeMap::new(),
                    event_of(edge, "Ctrl+F1"),
                    true,
                ),
                (
                    "configured combo",
                    combo_config("Ctrl+Shift+1"),
                    event_of(edge, "Ctrl+Shift+1"),
                    true,
                ),
                (
                    "other combo",
                    combo_config("Ctrl+Shift+1"),
                    event_of(edge, "Ctrl+Shift+2"),
                    false,
                ),
                (
                    "a hand-typed combo in another case and modifier order",
                    combo_config("shift+ctrl+1"),
                    event_of(edge, "Ctrl+Shift+1"),
                    true,
                ),
                (
                    "other kind on the hotkey source",
                    BTreeMap::new(),
                    Event::new(
                        EventSource::Hotkey,
                        event_kinds::REGISTERED,
                        json!({ "combo": "Ctrl+A", "id": 1u32 }),
                    ),
                    false,
                ),
                (
                    "the opposite half of the same hold",
                    BTreeMap::new(),
                    event_of(edge.opposite(), "Ctrl+F1"),
                    false,
                ),
                (
                    "hotkey kind forged on another source",
                    BTreeMap::new(),
                    Event::new(
                        EventSource::Midi,
                        edge.kind_id(),
                        json!({ "combo": "Ctrl+A" }),
                    ),
                    false,
                ),
            ];

            for (case, config, event, expected) in cases {
                assert_eq!(
                    descriptor.matches_trigger(&config, &event),
                    expected,
                    "wrong verdict for {case} on {edge:?}"
                );
            }
        }
    }

    #[test]
    fn build_arg_stack_exposes_the_held_duration_only_on_the_release_edge() {
        for (edge, hold) in [
            (HotkeyEdge::Press, None),
            (HotkeyEdge::Release, Some(Variant::Int(1250))),
        ] {
            let stack = HotkeyEdgeDescriptor::new(edge).build_arg_stack(&event_of(edge, "Ctrl+F5"));
            assert_eq!(
                stack.get("hotkey.combo"),
                Some(&Variant::String("Ctrl+F5".to_owned()))
            );
            assert_eq!(stack.get("hotkey.id"), Some(&Variant::Int(1)));
            assert_eq!(stack.get("hotkey.hold_ms"), hold.as_ref(), "{edge:?}");
        }
    }

    #[test]
    fn output_schema_declares_the_held_duration_only_on_the_release_edge() {
        for (edge, declared_count) in [(HotkeyEdge::Press, 3), (HotkeyEdge::Release, 4)] {
            let schema = HotkeyEdgeDescriptor::new(edge).output_schema().unwrap();
            assert_eq!(schema.variables.len(), declared_count, "{edge:?}");
        }
    }

    #[test]
    fn condition_display_falls_back_to_any_hotkey_when_no_combo_is_configured() {
        let descriptor = HotkeyEdgeDescriptor::new(HotkeyEdge::Press);
        for (config, expected) in [
            (
                combo_config("Ctrl+Shift+A"),
                UserText::from(VerbatimText::new("Ctrl+Shift+A")),
            ),
            (BTreeMap::new(), messages::CONDITION_ANY_HOTKEY.into()),
        ] {
            assert_eq!(descriptor.condition_display(&config), expected);
        }
    }
}
