use super::audio_source;
use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
};

use crate::vocabulary::fields::audio as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(variables::source::NAME, fields::SOURCE_NAME),
    EventVariable::float(variables::source::BALANCE, fields::BALANCE),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const AUDIO_SOURCE_BALANCE_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::AUDIO_SOURCE_BALANCE_CHANGED,
    source: EventSource::Obs,
    label: "OBS audio source balance changed",
    summary: "Fires when the audio balance of an OBS input source changes.",
    search_text: "obs audio balance pan stereo source input",
    icon: "sliders",
    prefix: event_kinds::AUDIO_PREFIX,
    matcher: TriggerMatcher::KindWhere(
        event_kinds::AUDIO_SOURCE_BALANCE_CHANGED,
        audio_source::source_name_matches,
    ),
    condition: TriggerCondition::FromConfig(audio_source::condition_display),
    config_fields: audio_source::config_fields,
    variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::Event;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;

    #[test]
    fn balance_arg_stack_extracts_name_and_balance_as_float() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_balance_changed",
            json!({ "source_name": "Mic", "balance": 0.75 }),
        );
        let stack = AUDIO_SOURCE_BALANCE_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(stack.get("obs.source.balance"), Some(&Variant::Float(0.75)));
    }

    #[test]
    fn balance_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_balance_changed",
            json!({}),
        );
        let stack = AUDIO_SOURCE_BALANCE_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.balance").is_none());
    }
}
