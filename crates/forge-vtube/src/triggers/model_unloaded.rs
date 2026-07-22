use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::model as fields;

pub struct ModelUnloadedDescriptor;

impl TriggerKindDescriptor for ModelUnloadedDescriptor {
    fn id(&self) -> &str {
        "vtube.model.unloaded"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio model unloaded"
    }

    fn summary(&self) -> &str {
        "Fires when a VTube Studio model is unloaded."
    }

    fn search_text(&self) -> &str {
        "vtube model unloaded removed avatar"
    }

    fn icon_name(&self) -> &str {
        "user-minus"
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
        event.kind == "model.unloaded"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get(fields::MODEL_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "vtube.model.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(id) = event.payload.get(fields::MODEL_ID).and_then(|v| v.as_str()) {
            stack = stack.set("vtube.model.id".to_owned(), Variant::String(id.to_owned()));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "vtube.model.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Model name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "vtube.model.id".to_owned(),
                    kind: VariantKind::String,
                    label: "Model ID".to_owned(),
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
    fn matches_only_the_exact_unloaded_kind() {
        let d = ModelUnloadedDescriptor;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("model.unloaded", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("model.loaded", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("expression.state_changed", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_present_payload_keys() {
        let d = ModelUnloadedDescriptor;
        let stack = d.build_arg_stack(&event(
            "model.unloaded",
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
        let d = ModelUnloadedDescriptor;
        let stack = d.build_arg_stack(&event("model.unloaded", json!({ "model_id": "m-42" })));
        assert!(stack.get("vtube.model.name").is_none());
        assert_eq!(
            stack.get("vtube.model.id"),
            Some(&Variant::String("m-42".to_owned()))
        );
    }
}
