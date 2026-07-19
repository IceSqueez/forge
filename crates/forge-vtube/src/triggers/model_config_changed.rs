use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct ModelConfigChangedDescriptor;

impl TriggerKindDescriptor for ModelConfigChangedDescriptor {
    fn id(&self) -> &str {
        "vtube.model.config_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio model config changed"
    }

    fn summary(&self) -> &str {
        "Fires when the configuration of the active VTube Studio model changes."
    }

    fn search_text(&self) -> &str {
        "vtube model config changed settings avatar"
    }

    fn icon_name(&self) -> &str {
        "settings"
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
        event.kind == "model.config_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("model_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "vtube.model.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "vtube.model.name".to_owned(),
                kind: VariantKind::String,
                label: "Model name".to_owned(),
                synthesis: None,
            }],
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
    fn matches_only_the_exact_config_changed_kind() {
        let d = ModelConfigChangedDescriptor;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("model.config_changed", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("model.loaded", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("hotkey.triggered", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_model_name() {
        let d = ModelConfigChangedDescriptor;
        let stack = d.build_arg_stack(&event(
            "model.config_changed",
            json!({ "model_name": "Aria" }),
        ));
        assert_eq!(
            stack.get("vtube.model.name"),
            Some(&Variant::String("Aria".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_model_name() {
        let d = ModelConfigChangedDescriptor;
        let stack = d.build_arg_stack(&event("model.config_changed", json!({})));
        assert!(stack.get("vtube.model.name").is_none());
    }
}
