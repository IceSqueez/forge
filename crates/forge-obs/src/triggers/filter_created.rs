use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

use super::filter_removed::build_filter_source_arg_stack;

pub struct FilterCreatedDescriptor;

impl TriggerKindDescriptor for FilterCreatedDescriptor {
    fn id(&self) -> &str {
        "obs.filters.created"
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
            kind_prefix: Some("filter.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "filter.created"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = build_filter_source_arg_stack(event);
        if let Some(kind) = event.payload.get("filter_kind").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.filter.kind".to_owned(),
                Variant::String(kind.to_owned()),
            );
        }
        stack
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // Source/filter-name extraction and kind discrimination are covered in `filter_removed`
    // (where the shared helper lives). Here we cover only the extra `obs.filter.kind` field
    // this descriptor layers on top of the shared stack.
    use super::*;
    use serde_json::json;

    #[test]
    fn arg_stack_extracts_filter_kind() {
        let event = Event::new(
            EventSource::Obs,
            "filter.created",
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
        let event = Event::new(EventSource::Obs, "filter.created", json!({}));
        assert!(
            FilterCreatedDescriptor
                .build_arg_stack(&event)
                .get("obs.filter.kind")
                .is_none()
        );
    }
}
