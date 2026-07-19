use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct HotkeyTriggeredDescriptor;

impl TriggerKindDescriptor for HotkeyTriggeredDescriptor {
    fn id(&self) -> &str {
        "vtube.hotkey.triggered"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio hotkey triggered"
    }

    fn summary(&self) -> &str {
        "Fires when a VTube Studio hotkey is activated."
    }

    fn search_text(&self) -> &str {
        "vtube hotkey triggered activated shortcut"
    }

    fn icon_name(&self) -> &str {
        "zap"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any hotkey".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some("hotkey.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "hotkey.triggered"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("hotkey_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "vtube.hotkey.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(id) = event.payload.get("hotkey_id").and_then(|v| v.as_str()) {
            stack = stack.set("vtube.hotkey.id".to_owned(), Variant::String(id.to_owned()));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "vtube.hotkey.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Hotkey name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "vtube.hotkey.id".to_owned(),
                    kind: VariantKind::String,
                    label: "Hotkey ID".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> Event {
        Event::new(EventSource::VTube, kind, payload)
    }

    #[test]
    fn matches_only_the_exact_triggered_kind() {
        let d = HotkeyTriggeredDescriptor;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("hotkey.triggered", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("model.loaded", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_payload_keys() {
        let d = HotkeyTriggeredDescriptor;
        let stack = d.build_arg_stack(&event(
            "hotkey.triggered",
            json!({ "hotkey_name": "Wave", "hotkey_id": "hk-7" }),
        ));
        assert_eq!(
            stack.get("vtube.hotkey.name"),
            Some(&Variant::String("Wave".to_owned()))
        );
        assert_eq!(
            stack.get("vtube.hotkey.id"),
            Some(&Variant::String("hk-7".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_payload_keys() {
        let d = HotkeyTriggeredDescriptor;
        let stack = d.build_arg_stack(&event("hotkey.triggered", json!({ "hotkey_id": "hk-7" })));
        assert!(stack.get("vtube.hotkey.name").is_none());
        assert_eq!(
            stack.get("vtube.hotkey.id"),
            Some(&Variant::String("hk-7".to_owned()))
        );
    }
}
