use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig};

use super::record_starting::build_record_arg_stack;

pub struct RecordStoppingDescriptor;

impl TriggerKindDescriptor for RecordStoppingDescriptor {
    fn id(&self) -> &str {
        "obs.record.stopping"
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

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "recording stopping".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("recording.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "recording.stopping"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_record_arg_stack(event)
    }
}
