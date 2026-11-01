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
    EventVariable::float(variables::source::VOLUME_DB, fields::VOLUME_DB),
    EventVariable::float(
        variables::source::VOLUME_MULTIPLIER,
        fields::VOLUME_MULTIPLIER,
    ),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const AUDIO_SOURCE_VOLUME_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::AUDIO_SOURCE_VOLUME_CHANGED,
    source: EventSource::Obs,
    label: "OBS audio source volume changed",
    summary: "Fires when the volume level of an OBS input source changes.",
    search_text: "obs audio volume db decibel source input level",
    icon: "volume",
    prefix: event_kinds::AUDIO_PREFIX,
    matcher: TriggerMatcher::KindWhere(
        event_kinds::AUDIO_SOURCE_VOLUME_CHANGED,
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
    fn volume_arg_stack_extracts_name_db_and_multiplier_as_floats() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_volume_changed",
            json!({ "source_name": "Mic", "volume_db": -12.5, "volume_multiplier": 0.25 }),
        );
        let stack = AUDIO_SOURCE_VOLUME_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.volume_db"),
            Some(&Variant::Float(-12.5)),
        );
        assert_eq!(
            stack.get("obs.source.volume_multiplier"),
            Some(&Variant::Float(0.25)),
        );
    }

    #[test]
    fn volume_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_volume_changed",
            json!({}),
        );
        let stack = AUDIO_SOURCE_VOLUME_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.volume_db").is_none());
        assert!(stack.get("obs.source.volume_multiplier").is_none());
    }
}
