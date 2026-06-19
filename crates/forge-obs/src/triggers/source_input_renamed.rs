use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct SourceInputRenamedDescriptor;

impl TriggerKindDescriptor for SourceInputRenamedDescriptor {
    fn id(&self) -> &str {
        "obs.sources.input_renamed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS input source renamed"
    }

    fn summary(&self) -> &str {
        "Fires when an input source is renamed in OBS."
    }

    fn search_text(&self) -> &str {
        "obs input source renamed name changed"
    }

    fn icon_name(&self) -> &str {
        "pencil"
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
        "any input renamed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("source.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "source.input_renamed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get("source_name_old")
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.name_old".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(name) = event
            .payload
            .get("source_name_new")
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.source.name_new".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        stack
    }
}
