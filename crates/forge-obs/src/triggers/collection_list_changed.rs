use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::collection as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct CollectionListChangedDescriptor;

impl TriggerKindDescriptor for CollectionListChangedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::COLLECTION_LIST_CHANGED
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

    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        messages::CONDITION_ANY_COLLECTION_LIST_CHANGE.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::COLLECTION_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::COLLECTION_LIST_CHANGED
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
                variables::collection::ALL_NAMES.to_owned(),
                Variant::Array(collections),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: variables::collection::ALL_NAMES.to_owned(),
                kind: VariantKind::Array,
                label: "All scene collection names".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
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
