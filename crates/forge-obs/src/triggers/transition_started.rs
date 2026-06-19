use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct TransitionStartedDescriptor;

impl TriggerKindDescriptor for TransitionStartedDescriptor {
    fn id(&self) -> &str {
        "obs.transition.started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene transition started"
    }

    fn summary(&self) -> &str {
        "Fires when a scene transition begins in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene transition started begin fade cut stinger"
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
        "any transition started".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("transition.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "transition.started"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_transition_arg_stack(event)
    }
}

pub(crate) fn build_transition_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(name) = event
        .payload
        .get("transition_name")
        .and_then(|v| v.as_str())
    {
        stack = stack.set(
            "obs.transition.name".to_owned(),
            Variant::String(name.to_owned()),
        );
    }
    stack
}
