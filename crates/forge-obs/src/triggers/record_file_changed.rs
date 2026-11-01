use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::recording as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text(
    variables::recording::OUTPUT_PATH,
    "New recording file path",
    fields::OUTPUT_PATH,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const RECORD_FILE_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::RECORDING_FILE_CHANGED,
    source: EventSource::Obs,
    label: "OBS recording file changed",
    summary: "Fires when OBS switches to a new output file mid-session (e.g. automatic file split).",
    search_text: "obs recording file changed split new path",
    icon: "record",
    prefix: event_kinds::RECORDING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::RECORDING_FILE_CHANGED),
    condition: TriggerCondition::Fixed(messages::CONDITION_RECORDING_FILE_CHANGED),
    config_fields: no_config_fields,
    variables,
};
