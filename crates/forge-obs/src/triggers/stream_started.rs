use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

use super::stream_starting::{build_stream_arg_stack, stream_variables};

pub struct StreamStartedDescriptor;

impl TriggerKindDescriptor for StreamStartedDescriptor {
    fn id(&self) -> &str {
        "obs.stream.started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS stream started"
    }

    fn summary(&self) -> &str {
        "Fires when OBS stream output becomes active and data is flowing."
    }

    fn search_text(&self) -> &str {
        "obs stream started live on air"
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
        "stream started".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("streaming.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "streaming.started"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_stream_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: stream_variables(),
        })
    }
}
