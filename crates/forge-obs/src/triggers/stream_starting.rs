use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const STREAM_STARTING: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::STREAMING_STARTING,
    source: EventSource::Obs,
    label: "OBS stream starting",
    summary: "Fires when OBS begins the stream start sequence (before output is active).",
    search_text: "obs stream starting go live begin",
    icon: "broadcast",
    prefix: event_kinds::STREAMING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::STREAMING_STARTING),
    condition: TriggerCondition::Fixed(messages::CONDITION_STREAM_STARTING),
    config_fields: no_config_fields,
    variables: families::stream_variables,
};

#[cfg(test)]
mod tests {
    use super::super::lifecycle;
    use super::super::stream_starting::STREAM_STARTING;
    use super::super::stream_status_changed::STREAM_STATUS_CHANGED;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    const ALL_STREAMING_KINDS: [&str; 6] = [
        "obs.streaming.starting",
        "obs.streaming.started",
        "obs.streaming.stopping",
        "obs.streaming.stopped",
        "obs.streaming.reconnecting",
        "obs.streaming.reconnected",
    ];

    fn stream_event(kind: &str, active: bool) -> Event {
        Event::new(EventSource::Obs, kind, json!({ "is_active": active }))
    }

    #[test]
    fn each_specific_descriptor_matches_only_its_own_kind() {
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 4] = [
            ("obs.streaming.starting", &STREAM_STARTING),
            ("obs.streaming.started", &lifecycle::STREAMING_STARTED),
            ("obs.streaming.stopping", &lifecycle::STREAMING_STOPPING),
            ("obs.streaming.stopped", &lifecycle::STREAMING_STOPPED),
        ];
        let cfg = BTreeMap::new();
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_STREAMING_KINDS {
                let event = stream_event(kind, true);
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &event),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
        }
    }

    #[test]
    fn omnibus_matches_every_streaming_kind() {
        let cfg = BTreeMap::new();
        for kind in ALL_STREAMING_KINDS {
            assert!(
                STREAM_STATUS_CHANGED.matches_trigger(&cfg, &stream_event(kind, true)),
                "omnibus should match {kind}",
            );
        }
    }

    #[test]
    fn omnibus_rejects_non_streaming_kind() {
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!STREAM_STATUS_CHANGED.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_takes_the_state_from_the_kind_and_is_active_from_the_payload() {
        let stack = STREAM_STARTING.build_arg_stack(&stream_event("obs.streaming.stopped", false));
        assert_eq!(
            stack.get("obs.stream.output_state"),
            Some(&Variant::String("stopped".to_owned())),
        );
        assert_eq!(
            stack.get("obs.stream.is_active"),
            Some(&Variant::Bool(false))
        );
    }

    #[test]
    fn build_arg_stack_derives_output_state_from_kind_when_payload_empty() {
        let event = Event::new(EventSource::Obs, "obs.streaming.started", json!({}));
        let stack = STREAM_STARTING.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.stream.output_state"),
            Some(&Variant::String("started".to_owned())),
        );
        assert!(stack.get("obs.stream.is_active").is_none());
    }
}
