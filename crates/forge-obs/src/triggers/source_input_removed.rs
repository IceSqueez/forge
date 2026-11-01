use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::source as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[EventVariable::text(
    variables::source::NAME,
    "Source name",
    fields::SOURCE_NAME,
)];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SOURCE_INPUT_REMOVED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SOURCE_INPUT_REMOVED,
    source: EventSource::Obs,
    label: "OBS input source removed",
    summary: "Fires when an input source is removed from OBS.",
    search_text: "obs input source removed deleted",
    icon: "minus-circle",
    prefix: event_kinds::SOURCE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SOURCE_INPUT_REMOVED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_INPUT_REMOVED),
    config_fields: no_config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;

    #[test]
    fn input_removed_arg_stack_extracts_source_name() {
        let event = Event::new(
            EventSource::Obs,
            "obs.source.input_removed",
            json!({ "source_name": "Webcam" }),
        );
        let stack = SOURCE_INPUT_REMOVED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Webcam".to_owned())),
        );
    }

    #[test]
    fn input_removed_arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.source.input_removed", json!({}));
        let stack = SOURCE_INPUT_REMOVED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
    }
}
