use super::families;
use forge_events::Event;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};
use forge_types::TriggerConfig;

use crate::vocabulary::{event_kinds, messages, trigger_ids};

fn matches(_config: &TriggerConfig, event: &Event) -> bool {
    matches!(
        event.kind.as_str(),
        event_kinds::RECORDING_STARTING
            | event_kinds::RECORDING_STARTED
            | event_kinds::RECORDING_STOPPING
            | event_kinds::RECORDING_STOPPED
            | event_kinds::RECORDING_PAUSED
            | event_kinds::RECORDING_RESUMED
    )
}

pub(crate) const RECORD_STATUS_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::RECORDING_STATUS_CHANGED,
    source: EventSource::Obs,
    label: "OBS recording status changed",
    summary: "Fires on any recording output state transition (starting, started, stopping, stopped, paused, resumed).",
    search_text: "obs recording status changed any transition lifecycle",
    icon: "record",
    prefix: event_kinds::RECORDING_PREFIX,
    matcher: TriggerMatcher::Custom(matches),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_RECORDING_STATUS_CHANGE),
    config_fields: no_config_fields,
    variables: families::record_variables,
};
