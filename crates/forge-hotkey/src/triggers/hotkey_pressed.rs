use std::collections::BTreeMap;

use super::combo_condition;
use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::{event_kinds, fields, trigger_ids, variables};

pub(crate) struct HotkeyPressedDescriptor;

impl TriggerKindDescriptor for HotkeyPressedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::GLOBAL_PRESSED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Hotkey
    }

    fn label(&self) -> &str {
        "Hotkey Pressed"
    }

    fn summary(&self) -> &str {
        "Fires when a registered global hotkey combo is pressed."
    }

    fn search_text(&self) -> &str {
        "hotkey global shortcut combo keyboard pressed"
    }

    fn icon_name(&self) -> &str {
        "keyboard"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
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
        if event.kind != event_kinds::GLOBAL_PRESSED {
            return false;
        }
        if event.source != EventSource::Hotkey {
            return false;
        }
        super::config_accepts_combo(config, event)
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(combo) = event.payload.get(fields::COMBO).and_then(|v| v.as_str()) {
            stack = stack.set(
                variables::COMBO.to_owned(),
                Variant::String(combo.to_owned()),
            );
        }
        if let Some(id) = event.payload.get(fields::ID).and_then(|v| v.as_u64()) {
            stack = stack.set(
                variables::ID.to_owned(),
                Variant::Int(i64::try_from(id).unwrap_or(i64::MAX)),
            );
        }
        if let Some(ts) = event
            .payload
            .get(fields::TIMESTAMP_US)
            .and_then(|v| v.as_u64())
        {
            stack = stack.set(
                variables::TIMESTAMP_US.to_owned(),
                Variant::Int(i64::try_from(ts).unwrap_or(i64::MAX)),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: variables::COMBO.to_owned(),
                    kind: VariantKind::String,
                    label: "Hotkey combo".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: variables::ID.to_owned(),
                    kind: VariantKind::Int,
                    label: "Hotkey ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: variables::TIMESTAMP_US.to_owned(),
                    kind: VariantKind::Int,
                    label: "Timestamp (microseconds)".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::messages;
    use forge_events::EventSource;
    use forge_types::VerbatimText;
    use serde_json::json;

    fn triggered_event(combo: &str) -> Event {
        Event::new(
            EventSource::Hotkey,
            "hotkey.global.pressed",
            json!({ "combo": combo, "id": 1u32, "timestamp_us": 0u64 }),
        )
    }

    fn combo_config(combo: &str) -> BTreeMap<String, Variant> {
        BTreeMap::from([("combo".to_owned(), Variant::String(combo.to_owned()))])
    }

    #[test]
    fn matches_trigger_admits_only_a_hotkey_press_whose_combo_the_config_accepts() {
        let cases = [
            (
                "empty config takes any combo",
                BTreeMap::new(),
                triggered_event("Ctrl+F1"),
                true,
            ),
            (
                "configured combo",
                combo_config("Ctrl+Shift+1"),
                triggered_event("Ctrl+Shift+1"),
                true,
            ),
            (
                "other combo",
                combo_config("Ctrl+Shift+1"),
                triggered_event("Ctrl+Shift+2"),
                false,
            ),
            (
                "a hand-typed combo in another case and modifier order",
                combo_config("shift+ctrl+1"),
                triggered_event("Ctrl+Shift+1"),
                true,
            ),
            (
                "other kind on the hotkey source",
                BTreeMap::new(),
                Event::new(
                    EventSource::Hotkey,
                    "hotkey.registered",
                    json!({ "combo": "Ctrl+A", "id": 1u32 }),
                ),
                false,
            ),
            (
                "the release half of the same hold",
                BTreeMap::new(),
                Event::new(
                    EventSource::Hotkey,
                    "hotkey.global.released",
                    json!({ "combo": "Ctrl+F1", "id": 1u32, "hold_ms": 10u64 }),
                ),
                false,
            ),
            (
                "hotkey kind forged on another source",
                BTreeMap::new(),
                Event::new(
                    EventSource::Midi,
                    "hotkey.global.pressed",
                    json!({ "combo": "Ctrl+A" }),
                ),
                false,
            ),
        ];

        for (case, config, event, expected) in cases {
            assert_eq!(
                HotkeyPressedDescriptor.matches_trigger(&config, &event),
                expected,
                "wrong verdict for {case}"
            );
        }
    }

    #[test]
    fn build_arg_stack_populates_hotkey_keys() {
        let ev = triggered_event("Ctrl+F5");
        let stack = HotkeyPressedDescriptor.build_arg_stack(&ev);
        assert_eq!(
            stack.get("hotkey.combo"),
            Some(&Variant::String("Ctrl+F5".to_owned()))
        );
        assert_eq!(stack.get("hotkey.id"), Some(&Variant::Int(1)));
    }

    #[test]
    fn condition_display_falls_back_to_any_hotkey_when_no_combo_is_configured() {
        for (config, expected) in [
            (
                combo_config("Ctrl+Shift+A"),
                UserText::from(VerbatimText::new("Ctrl+Shift+A")),
            ),
            (BTreeMap::new(), messages::CONDITION_ANY_HOTKEY.into()),
        ] {
            assert_eq!(HotkeyPressedDescriptor.condition_display(&config), expected);
        }
    }
}
