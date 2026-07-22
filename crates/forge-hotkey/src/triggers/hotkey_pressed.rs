use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields as fields;

pub struct HotkeyPressedDescriptor;

impl TriggerKindDescriptor for HotkeyPressedDescriptor {
    fn id(&self) -> &str {
        "hotkey.global.pressed"
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
            key: "combo",
            label: "Hotkey combo",
            placeholder: "e.g. Ctrl+Shift+1",
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        config
            .get("combo")
            .and_then(|v| {
                if let Variant::String(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "any hotkey".to_owned())
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Hotkey),
            kind_prefix: Some("hotkey.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "hotkey.global.pressed" {
            return false;
        }
        if event.source != EventSource::Hotkey {
            return false;
        }
        if let Some(Variant::String(combo)) = config.get("combo") {
            let event_combo = event
                .payload
                .get(fields::COMBO)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return combo == event_combo;
        }
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(combo) = event.payload.get(fields::COMBO).and_then(|v| v.as_str()) {
            stack = stack.set("hotkey.combo".to_owned(), Variant::String(combo.to_owned()));
        }
        if let Some(id) = event.payload.get(fields::ID).and_then(|v| v.as_u64()) {
            stack = stack.set("hotkey.id".to_owned(), Variant::Int(id as i64));
        }
        if let Some(ts) = event
            .payload
            .get(fields::TIMESTAMP_US)
            .and_then(|v| v.as_u64())
        {
            stack = stack.set("hotkey.timestamp_us".to_owned(), Variant::Int(ts as i64));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "hotkey.combo".to_owned(),
                    kind: VariantKind::String,
                    label: "Hotkey combo".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "hotkey.id".to_owned(),
                    kind: VariantKind::Int,
                    label: "Hotkey ID".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "hotkey.timestamp_us".to_owned(),
                    kind: VariantKind::Int,
                    label: "Timestamp (microseconds)".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use serde_json::json;

    fn triggered_event(combo: &str) -> Event {
        Event::new(
            EventSource::Hotkey,
            "hotkey.global.pressed",
            json!({ "combo": combo, "id": 1u32, "timestamp_us": 0u64 }),
        )
    }

    #[test]
    fn matches_any_when_config_empty() {
        let ev = triggered_event("Ctrl+F1");
        assert!(HotkeyPressedDescriptor.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn matches_exact_combo() {
        let ev = triggered_event("Ctrl+Shift+1");
        let cfg = BTreeMap::from([(
            "combo".to_owned(),
            Variant::String("Ctrl+Shift+1".to_owned()),
        )]);
        assert!(HotkeyPressedDescriptor.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn rejects_different_combo() {
        let ev = triggered_event("Ctrl+Shift+2");
        let cfg = BTreeMap::from([(
            "combo".to_owned(),
            Variant::String("Ctrl+Shift+1".to_owned()),
        )]);
        assert!(!HotkeyPressedDescriptor.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn does_not_match_wrong_kind() {
        let ev = Event::new(
            EventSource::Hotkey,
            "hotkey.registered",
            json!({ "combo": "Ctrl+A", "id": 1u32 }),
        );
        assert!(!HotkeyPressedDescriptor.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_wrong_source() {
        let ev = Event::new(
            EventSource::Midi,
            "hotkey.global.pressed",
            json!({ "combo": "Ctrl+A" }),
        );
        assert!(!HotkeyPressedDescriptor.matches_trigger(&BTreeMap::new(), &ev));
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
    fn condition_display_shows_combo() {
        let cfg = BTreeMap::from([(
            "combo".to_owned(),
            Variant::String("Ctrl+Shift+A".to_owned()),
        )]);
        assert_eq!(
            HotkeyPressedDescriptor.condition_display(&cfg),
            "Ctrl+Shift+A"
        );
    }

    #[test]
    fn condition_display_any_when_no_config() {
        assert_eq!(
            HotkeyPressedDescriptor.condition_display(&BTreeMap::new()),
            "any hotkey"
        );
    }

    #[test]
    fn event_filter_source_is_hotkey() {
        let filter = HotkeyPressedDescriptor.event_filter();
        assert_eq!(filter.source, Some(EventSource::Hotkey));
    }
}
