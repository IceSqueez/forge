use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, UserText, VariableSchema};

use super::record_starting::{build_record_arg_stack, record_variables};

use crate::vocabulary::{event_kinds, messages, trigger_ids};

pub struct RecordStoppingDescriptor;

impl TriggerKindDescriptor for RecordStoppingDescriptor {
    fn id(&self) -> &str {
        trigger_ids::RECORDING_STOPPING
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS recording stopping"
    }

    fn summary(&self) -> &str {
        "Fires when OBS begins the recording stop sequence (before output becomes inactive)."
    }

    fn search_text(&self) -> &str {
        "obs recording stopping end capture finalise"
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

    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        messages::CONDITION_RECORDING_STOPPING.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::RECORDING_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::RECORDING_STOPPING
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
