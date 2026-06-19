use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct StreamStartingDescriptor;

impl TriggerKindDescriptor for StreamStartingDescriptor {
    fn id(&self) -> &str {
        "obs.stream.starting"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS stream starting"
    }

    fn summary(&self) -> &str {
        "Fires when OBS begins the stream start sequence (before output is active)."
    }

    fn search_text(&self) -> &str {
        "obs stream starting go live begin"
    }

    fn icon_name(&self) -> &str {
        "broadcast"
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
        "stream starting".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("streaming.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "streaming.starting"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_stream_arg_stack(event)
    }
}

pub(crate) fn build_stream_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(s) = event.payload.get("output_state").and_then(|v| v.as_str()) {
        stack = stack.set(
            "obs.stream.output_state".to_owned(),
            Variant::String(s.to_owned()),
        );
    }
    if let Some(b) = event.payload.get("is_active").and_then(|v| v.as_bool()) {
        stack = stack.set("obs.stream.is_active".to_owned(), Variant::Bool(b));
    }
    stack
}
