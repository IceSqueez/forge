use super::families;
use forge_events::Event;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};
use forge_types::TriggerConfig;

use crate::vocabulary::{event_kinds, messages, trigger_ids};

fn matches(_config: &TriggerConfig, event: &Event) -> bool {
    event.kind.starts_with(event_kinds::STREAMING_PREFIX)
}

pub(crate) const STREAM_STATUS_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::STREAMING_STATUS_CHANGED,
    source: EventSource::Obs,
    label: "OBS stream status changed",
    summary: "Fires on any stream output state transition (starting, started, stopping, stopped, reconnecting, reconnected).",
    search_text: "obs stream status changed any transition lifecycle",
    icon: "broadcast",
    prefix: event_kinds::STREAMING_PREFIX,
    matcher: TriggerMatcher::Custom(matches),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_STREAM_STATUS_CHANGE),
    config_fields: no_config_fields,
    variables: families::stream_variables,
};
