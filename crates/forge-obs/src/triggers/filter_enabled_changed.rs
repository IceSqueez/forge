use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use super::filter_removed::{build_filter_source_arg_stack, filter_source_variables};
use crate::payload_fields::filter as fields;

pub struct FilterEnabledChangedDescriptor;

impl TriggerKindDescriptor for FilterEnabledChangedDescriptor {
    fn id(&self) -> &str {
        "obs.filters.enabled_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS filter enable state changed"
    }

    fn summary(&self) -> &str {
        "Fires when a filter on an OBS source is enabled or disabled."
    }

    fn search_text(&self) -> &str {
        "obs filter enabled disabled toggled source"
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
        "any filter enable state changed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.filter.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.filter.enabled_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = build_filter_source_arg_stack(event);
        if let Some(enabled) = event
            .payload
            .get(fields::IS_ENABLED)
            .and_then(|v| v.as_bool())
        {
            stack = stack.set("obs.filter.is_enabled".to_owned(), Variant::Bool(enabled));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        let mut variables = filter_source_variables();
        variables.push(DeclaredVariable {
            name: "obs.filter.is_enabled".to_owned(),
            kind: VariantKind::Bool,
            label: "Filter enabled".to_owned(),
            synthesis: None,
        });
        Some(VariableSchema { variables })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn arg_stack_preserves_is_enabled_both_true_and_false() {
        for flag in [true, false] {
            let event = Event::new(
                EventSource::Obs,
                "obs.filter.enabled_changed",
                json!({ "is_enabled": flag }),
            );
            assert_eq!(
                FilterEnabledChangedDescriptor
                    .build_arg_stack(&event)
                    .get("obs.filter.is_enabled"),
                Some(&Variant::Bool(flag)),
            );
        }
    }

    #[test]
    fn arg_stack_omits_is_enabled_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.filter.enabled_changed", json!({}));
        assert!(
            FilterEnabledChangedDescriptor
                .build_arg_stack(&event)
                .get("obs.filter.is_enabled")
                .is_none()
        );
    }
}
