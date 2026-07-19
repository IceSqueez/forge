use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

use super::record_starting::{build_record_arg_stack, record_variables};

pub struct RecordStoppedDescriptor;

impl TriggerKindDescriptor for RecordStoppedDescriptor {
    fn id(&self) -> &str {
        "obs.record.stopped"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS recording stopped"
    }

    fn summary(&self) -> &str {
        "Fires when OBS recording output becomes inactive. The output_path arg carries the saved file path."
    }

    fn search_text(&self) -> &str {
        "obs recording stopped finished saved file path"
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
        "recording stopped".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("recording.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "recording.stopped"
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
