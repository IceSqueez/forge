use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::collection as fields;

pub struct SceneCollectionChangingDescriptor;

impl TriggerKindDescriptor for SceneCollectionChangingDescriptor {
    fn id(&self) -> &str {
        "obs.collection.changing"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene collection changing"
    }

    fn summary(&self) -> &str {
        "Fires when OBS begins switching to a different scene collection."
    }

    fn search_text(&self) -> &str {
        "obs scene collection changing switching"
    }

    fn icon_name(&self) -> &str {
        "stack-2"
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
        "any scene collection".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.collection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.collection.changing"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get(fields::COLLECTION_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.collection".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "obs.collection".to_owned(),
                kind: VariantKind::String,
                label: "Scene collection name".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_collection_changing_kind() {
        let d = SceneCollectionChangingDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.changing",
            json!({ "collection_name": "Stream" }),
        );
        assert!(d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn does_not_match_collection_changed_kind() {
        let d = SceneCollectionChangingDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.changed",
            json!({ "collection_name": "Stream" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_maps_name_to_collection() {
        let d = SceneCollectionChangingDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.changing",
            json!({ "collection_name": "Stream" }),
        );
        assert_eq!(
            d.build_arg_stack(&event).get("obs.collection"),
            Some(&Variant::String("Stream".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_collection_when_name_missing() {
        let d = SceneCollectionChangingDescriptor;
        let event = Event::new(EventSource::Obs, "obs.collection.changing", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.collection"), None);
    }
}
