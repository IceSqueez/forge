use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use super::filter_removed::{build_filter_source_arg_stack, filter_source_variables};
use crate::vocabulary::fields::filter as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub struct FilterCreatedDescriptor;

impl TriggerKindDescriptor for FilterCreatedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::FILTER_CREATED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS filter created"
    }

    fn summary(&self) -> &str {
        "Fires when a filter is added to an OBS source."
    }

    fn search_text(&self) -> &str {
        "obs filter created added source"
    }

    fn icon_name(&self) -> &str {
        "filter"
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
        "any filter created".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::FILTER_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::FILTER_CREATED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = build_filter_source_arg_stack(event);
        if let Some(kind) = event
            .payload
            .get(fields::FILTER_KIND)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                variables::filter::KIND.to_owned(),
                Variant::String(kind.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        let mut variables = filter_source_variables();
        variables.push(DeclaredVariable {
            name: variables::filter::KIND.to_owned(),
            kind: VariantKind::String,
            label: "Filter kind".to_owned(),
            synthesis: None,
        });
        Some(VariableSchema { variables })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn arg_stack_extracts_filter_kind() {
        let event = Event::new(
            EventSource::Obs,
            "obs.filter.created",
            json!({ "filter_kind": "noise_suppress_filter" }),
        );
        assert_eq!(
            FilterCreatedDescriptor
                .build_arg_stack(&event)
                .get("obs.filter.kind"),
            Some(&Variant::String("noise_suppress_filter".to_owned())),
        );
    }

    #[test]
    fn arg_stack_omits_filter_kind_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.filter.created", json!({}));
        assert!(
            FilterCreatedDescriptor
                .build_arg_stack(&event)
                .get("obs.filter.kind")
                .is_none()
        );
    }
}
