use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
    no_config_fields,
};

use crate::vocabulary::fields::source as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(variables::source::NAME_OLD, fields::SOURCE_NAME_OLD),
    EventVariable::text(variables::source::NAME_NEW, fields::SOURCE_NAME_NEW),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const SOURCE_INPUT_RENAMED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::SOURCE_INPUT_RENAMED,
    source: EventSource::Obs,
    label: "OBS input source renamed",
    summary: "Fires when an input source is renamed in OBS.",
    search_text: "obs input source renamed name changed",
    icon: "pencil",
    prefix: event_kinds::SOURCE_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::SOURCE_INPUT_RENAMED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_INPUT_RENAMED),
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
    fn input_renamed_arg_stack_extracts_old_and_new_names() {
        let event = Event::new(
            EventSource::Obs,
            "obs.source.input_renamed",
            json!({ "source_name_old": "Cam", "source_name_new": "Webcam" }),
        );
        let stack = SOURCE_INPUT_RENAMED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name_old"),
            Some(&Variant::String("Cam".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.name_new"),
            Some(&Variant::String("Webcam".to_owned())),
        );
    }

    #[test]
    fn input_renamed_arg_stack_extracts_each_name_independently() {
        let event = Event::new(
            EventSource::Obs,
            "obs.source.input_renamed",
            json!({ "source_name_new": "Webcam" }),
        );
        let stack = SOURCE_INPUT_RENAMED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name_old").is_none());
        assert_eq!(
            stack.get("obs.source.name_new"),
            Some(&Variant::String("Webcam".to_owned())),
        );
    }

    #[test]
    fn input_renamed_arg_stack_omits_both_keys_when_payload_empty() {
        let event = Event::new(EventSource::Obs, "obs.source.input_renamed", json!({}));
        let stack = SOURCE_INPUT_RENAMED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name_old").is_none());
        assert!(stack.get("obs.source.name_new").is_none());
    }
}
