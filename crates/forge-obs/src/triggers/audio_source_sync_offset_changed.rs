use super::audio_source;
use forge_events::EventSource;
use forge_registry::TriggerVariables;
use forge_registry::{
    EventTriggerSpec, EventVariable, TriggerCondition, TriggerMatcher, event_variables,
};
use forge_types::SynthesisHint;

use crate::vocabulary::fields::audio as fields;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

const SYNC_OFFSET_MIN_MS: i64 = -950;
const SYNC_OFFSET_MAX_MS: i64 = 20_000;

const VARIABLES: &[EventVariable] = &[
    EventVariable::text(variables::source::NAME, "Source name", fields::SOURCE_NAME),
    EventVariable::int(
        variables::source::SYNC_OFFSET_MS,
        "Sync offset (ms)",
        fields::SYNC_OFFSET_MS,
    )
    .synthesis(SynthesisHint::BoundedInt {
        min: SYNC_OFFSET_MIN_MS,
        max: SYNC_OFFSET_MAX_MS,
    }),
];

fn variables() -> Option<TriggerVariables> {
    event_variables(VARIABLES)
}

pub(crate) const AUDIO_SOURCE_SYNC_OFFSET_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::AUDIO_SOURCE_SYNC_OFFSET_CHANGED,
    source: EventSource::Obs,
    label: "OBS audio source sync offset changed",
    summary: "Fires when the audio sync offset of an OBS input source changes.",
    search_text: "obs audio sync offset delay source input milliseconds",
    icon: "timer",
    prefix: event_kinds::AUDIO_PREFIX,
    matcher: TriggerMatcher::KindWhere(
        event_kinds::AUDIO_SOURCE_SYNC_OFFSET_CHANGED,
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
    fn sync_offset_arg_stack_extracts_name_and_offset_as_int() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_sync_offset_changed",
            json!({ "source_name": "Mic", "sync_offset_ms": -120 }),
        );
        let stack = AUDIO_SOURCE_SYNC_OFFSET_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.source.name"),
            Some(&Variant::String("Mic".to_owned())),
        );
        assert_eq!(
            stack.get("obs.source.sync_offset_ms"),
            Some(&Variant::Int(-120)),
        );
    }

    #[test]
    fn sync_offset_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(
            EventSource::Obs,
            "obs.audio.source_sync_offset_changed",
            json!({}),
        );
        let stack = AUDIO_SOURCE_SYNC_OFFSET_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.source.name").is_none());
        assert!(stack.get("obs.source.sync_offset_ms").is_none());
    }
}
