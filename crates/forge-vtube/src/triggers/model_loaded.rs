use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct ModelLoadedDescriptor;

impl TriggerKindDescriptor for ModelLoadedDescriptor {
    fn id(&self) -> &str {
        "vtube.model.loaded"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio model loaded"
    }

    fn summary(&self) -> &str {
        "Fires when a VTube Studio model is loaded."
    }

    fn search_text(&self) -> &str {
        "vtube model loaded avatar switch"
    }

    fn icon_name(&self) -> &str {
        "user"
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
        "any model".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some("model.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "model.loaded"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("model_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "vtube.model.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(id) = event.payload.get("model_id").and_then(|v| v.as_str()) {
            stack = stack.set("vtube.model.id".to_owned(), Variant::String(id.to_owned()));
        }
        stack
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
    fn matches_only_the_exact_loaded_kind() {
        let d = ModelLoadedDescriptor;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("model.loaded", json!({}))));
        // Sibling under the same `model.` prefix must not match.
        assert!(!d.matches_trigger(&cfg, &event("model.unloaded", json!({}))));
        // Foreign kind.
        assert!(!d.matches_trigger(&cfg, &event("hotkey.triggered", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_payload_keys() {
        let d = ModelLoadedDescriptor;
        let stack = d.build_arg_stack(&event(
            "model.loaded",
            json!({ "model_name": "Aria", "model_id": "m-42" }),
        ));
        assert_eq!(
            stack.get("vtube.model.name"),
            Some(&Variant::String("Aria".to_owned()))
        );
        assert_eq!(
            stack.get("vtube.model.id"),
            Some(&Variant::String("m-42".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_payload_keys() {
        let d = ModelLoadedDescriptor;
        let stack = d.build_arg_stack(&event("model.loaded", json!({ "model_name": "Aria" })));
        assert_eq!(
            stack.get("vtube.model.name"),
            Some(&Variant::String("Aria".to_owned()))
        );
        assert!(stack.get("vtube.model.id").is_none());
    }
}
