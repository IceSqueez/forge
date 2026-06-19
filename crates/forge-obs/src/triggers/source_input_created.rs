use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SourceInputCreatedDescriptor;

impl TriggerKindDescriptor for SourceInputCreatedDescriptor {
    fn id(&self) -> &str {
        "obs.sources.input_created"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS input source created"
    }

    fn summary(&self) -> &str {
        "Fires when a new input source is added to OBS."
    }

    fn search_text(&self) -> &str {
        "obs input source created added new"
    }

    fn icon_name(&self) -> &str {
        "plus-circle"
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
        "any input created".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("source.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "source.input_created"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("source_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.source.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(kind) = event.payload.get("source_kind").and_then(|v| v.as_str()) {
            stack = stack.set(
                "obs.source.kind".to_owned(),
                Variant::String(kind.to_owned()),
            );
        }
        stack
    }
}
