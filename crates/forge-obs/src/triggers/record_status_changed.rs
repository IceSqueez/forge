use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

use super::record_starting::{build_record_arg_stack, record_variables};

pub struct RecordStatusChangedDescriptor;

impl TriggerKindDescriptor for RecordStatusChangedDescriptor {
    fn id(&self) -> &str {
        "obs.record.status_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS recording status changed"
    }

    fn summary(&self) -> &str {
        "Fires on any recording output state transition (starting, started, stopping, stopped, paused, resumed)."
    }

    fn search_text(&self) -> &str {
        "obs recording status changed any transition lifecycle"
    }

    fn icon_name(&self) -> &str {
        "record"
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
        "any recording status change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("recording.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        matches!(
            event.kind.as_str(),
            "recording.starting"
                | "recording.started"
                | "recording.stopping"
                | "recording.stopped"
                | "recording.paused"
                | "recording.resumed"
        )
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_record_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: record_variables(),
        })
    }
}
