use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

use super::transition_started::{build_transition_arg_stack, transition_variables};

pub struct TransitionEndedDescriptor;

impl TriggerKindDescriptor for TransitionEndedDescriptor {
    fn id(&self) -> &str {
        "obs.transition.ended"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene transition ended"
    }

    fn summary(&self) -> &str {
        "Fires when a scene transition completes in OBS (cut point; video may still play)."
    }

    fn search_text(&self) -> &str {
        "obs scene transition ended complete finished fade cut stinger"
    }

    fn icon_name(&self) -> &str {
        "arrow-right"
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
        "any transition ended".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("transition.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "transition.ended"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_transition_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: transition_variables(),
        })
    }
}
