use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SceneCollectionChangedDescriptor;

impl TriggerKindDescriptor for SceneCollectionChangedDescriptor {
    fn id(&self) -> &str {
        "obs.collection.current_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene collection changed"
    }

    fn summary(&self) -> &str {
        "Fires after OBS finishes switching to a different scene collection."
    }

    fn search_text(&self) -> &str {
        "obs scene collection changed switched"
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
            kind_prefix: Some("collection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "collection.changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.collection".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_collection_changed_kind() {
        let d = SceneCollectionChangedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "collection.changed",
            json!({ "name": "Stream" }),
        );
        assert!(d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn does_not_match_collection_changing_kind() {
        let d = SceneCollectionChangedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "collection.changing",
            json!({ "name": "Stream" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_maps_name_to_collection() {
        let d = SceneCollectionChangedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "collection.changed",
            json!({ "name": "Stream" }),
        );
        assert_eq!(
            d.build_arg_stack(&event).get("obs.collection"),
            Some(&Variant::String("Stream".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_collection_when_name_missing() {
        let d = SceneCollectionChangedDescriptor;
        let event = Event::new(EventSource::Obs, "collection.changed", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.collection"), None);
    }
}
