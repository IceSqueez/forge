use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::collection as fields;

pub struct CollectionListChangedDescriptor;

impl TriggerKindDescriptor for CollectionListChangedDescriptor {
    fn id(&self) -> &str {
        "obs.collection.list_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene collection list changed"
    }

    fn summary(&self) -> &str {
        "Fires when scene collections are added or removed in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene collection list changed added removed"
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
        "any collection list change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.collection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.collection.list_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(names) = event
            .payload
            .get(fields::ALL_NAMES)
            .and_then(|v| v.as_array())
        {
            let collections: Vec<Variant> = names
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| Variant::String(s.to_owned()))
                .collect();
            stack = stack.set(
                "obs.collection.all_names".to_owned(),
                Variant::Array(collections),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "obs.collection.all_names".to_owned(),
                kind: VariantKind::Array,
                label: "All scene collection names".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    #[test]
    fn matches_list_changed_and_rejects_sibling_collection_kinds() {
        let d = CollectionListChangedDescriptor;
        let cfg = BTreeMap::new();
        assert!(d.matches_trigger(
            &cfg,
            &Event::new(EventSource::Obs, "obs.collection.list_changed", json!({})),
        ));
        for sibling in ["obs.collection.changing", "obs.collection.changed"] {
            assert!(
                !d.matches_trigger(&cfg, &Event::new(EventSource::Obs, sibling, json!({}))),
                "obs.collection.list_changed wrongly matched sibling kind {sibling}",
            );
        }
    }

    #[test]
    fn arg_stack_collects_all_names_into_string_array() {
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.list_changed",
            json!({ "all_names": ["Main", "Alt"] }),
        );
        let stack = CollectionListChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.collection.all_names"),
            Some(&Variant::Array(vec![
                Variant::String("Main".to_owned()),
                Variant::String("Alt".to_owned()),
            ])),
        );
    }

    #[test]
    fn arg_stack_skips_non_string_array_elements() {
        let event = Event::new(
            EventSource::Obs,
            "obs.collection.list_changed",
            json!({ "all_names": ["Main", 7, null, "Alt"] }),
        );
        let stack = CollectionListChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.collection.all_names"),
            Some(&Variant::Array(vec![
                Variant::String("Main".to_owned()),
                Variant::String("Alt".to_owned()),
            ])),
        );
    }

    #[test]
    fn arg_stack_omits_key_when_all_names_absent() {
        let event = Event::new(EventSource::Obs, "obs.collection.list_changed", json!({}));
        let stack = CollectionListChangedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.collection.all_names").is_none());
    }
}
