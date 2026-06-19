use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

use super::filter_removed::build_filter_source_arg_stack;

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
            kind_prefix: Some("filter.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "filter.enabled_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = build_filter_source_arg_stack(event);
        if let Some(enabled) = event.payload.get("is_enabled").and_then(|v| v.as_bool()) {
            stack = stack.set("obs.filter.is_enabled".to_owned(), Variant::Bool(enabled));
        }
        stack
    }
}
