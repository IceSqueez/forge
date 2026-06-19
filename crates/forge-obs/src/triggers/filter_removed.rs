use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct FilterRemovedDescriptor;

impl TriggerKindDescriptor for FilterRemovedDescriptor {
    fn id(&self) -> &str {
        "obs.filters.removed"
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

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "any filter removed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("filter.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "filter.removed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_filter_source_arg_stack(event)
    }
}

pub(crate) fn build_filter_source_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(name) = event.payload.get("source_name").and_then(|v| v.as_str()) {
        stack = stack.set(
            "obs.source.name".to_owned(),
            Variant::String(name.to_owned()),
        );
    }
    if let Some(name) = event.payload.get("filter_name").and_then(|v| v.as_str()) {
        stack = stack.set(
            "obs.filter.name".to_owned(),
            Variant::String(name.to_owned()),
        );
    }
    stack
}
