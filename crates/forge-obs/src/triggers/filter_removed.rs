use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::filter as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct FilterRemovedDescriptor;

impl TriggerKindDescriptor for FilterRemovedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::FILTER_REMOVED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS filter removed"
    }

    fn summary(&self) -> &str {
        "Fires when a filter is removed from an OBS source."
    }

    fn search_text(&self) -> &str {
        "obs filter removed deleted source"
    }

    fn icon_name(&self) -> &str {
        "filter-x"
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
        messages::CONDITION_ANY_FILTER_REMOVED.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::FILTER_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::FILTER_REMOVED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_filter_source_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: filter_source_variables(),
        })
    }
}

pub(crate) fn filter_source_variables() -> Vec<DeclaredVariable> {
    vec![
        DeclaredVariable {
            name: variables::source::NAME.to_owned(),
            kind: VariantKind::String,
            label: "Source name".to_owned(),
            synthesis: None,
        },
        DeclaredVariable {
            name: variables::filter::NAME.to_owned(),
            kind: VariantKind::String,
            label: "Filter name".to_owned(),
            synthesis: None,
        },
    ]
}

pub(crate) fn build_filter_source_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(name) = event
        .payload
        .get(fields::SOURCE_NAME)
        .and_then(|v| v.as_str())
    {
        stack = stack.set(
            variables::source::NAME.to_owned(),
            Variant::String(name.to_owned()),
        );
    }
    if let Some(name) = event
        .payload
        .get(fields::FILTER_NAME)
        .and_then(|v| v.as_str())
    {
        stack = stack.set(
            variables::filter::NAME.to_owned(),
            Variant::String(name.to_owned()),
        );
    }
    stack
}

#[cfg(test)]
mod tests {
    use super::super::{
        FilterCreatedDescriptor, FilterEnabledChangedDescriptor, FilterRemovedDescriptor,
    };
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    const ALL_FILTER_KINDS: [&str; 3] = [
        "obs.filter.created",
        "obs.filter.removed",
        "obs.filter.enabled_changed",
    ];

    #[test]
    fn each_filter_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 3] = [
            ("obs.filter.created", &FilterCreatedDescriptor),
            ("obs.filter.removed", &FilterRemovedDescriptor),
            (
                "obs.filter.enabled_changed",
                &FilterEnabledChangedDescriptor,
            ),
        ];
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_FILTER_KINDS {
                let event = Event::new(EventSource::Obs, kind, json!({}));
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &event),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
        }
    }

    #[test]
    fn filter_descriptors_reject_non_filter_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [&dyn TriggerKindDescriptor; 3] = [
            &FilterCreatedDescriptor,
            &FilterRemovedDescriptor,
            &FilterEnabledChangedDescriptor,
        ];
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        for descriptor in descriptors {
            assert!(!descriptor.matches_trigger(&cfg, &event));
        }
    }

    #[test]
    fn filter_source_arg_stack_extracts_source_and_filter_names() {
        let event = Event::new(
            EventSource::Obs,
            "obs.filter.removed",
            json!({ "source_name": "Mic", "filter_name": "Noise Gate" }),
        );
        let stack = build_filter_source_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(
            stack.get("obs.filter.name"),
            Some(&Variant::String("Noise Gate".to_owned())),
        );
    }

    #[test]
    fn filter_source_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "obs.filter.removed", json!({}));
        let stack = build_filter_source_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.filter.name").is_none());
    }
}
