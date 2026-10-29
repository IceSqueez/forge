use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

use super::stream_starting::{build_stream_arg_stack, stream_variables};

use crate::vocabulary::{event_kinds, trigger_ids};

pub struct StreamStatusChangedDescriptor;

impl TriggerKindDescriptor for StreamStatusChangedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::STREAMING_STATUS_CHANGED
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS stream status changed"
    }

    fn summary(&self) -> &str {
        "Fires on any stream output state transition (starting, started, stopping, stopped, reconnecting, reconnected)."
    }

    fn search_text(&self) -> &str {
        "obs stream status changed any transition lifecycle"
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
        "any stream status change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::STREAMING_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind.starts_with(event_kinds::STREAMING_PREFIX)
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
