use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const TRANSITION_STARTED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::TRANSITION_STARTED,
    source: EventSource::Obs,
    label: "OBS scene transition started",
    summary: "Fires when a scene transition begins in OBS.",
    search_text: "obs scene transition started begin fade cut stinger",
    icon: "arrow-right",
    prefix: event_kinds::TRANSITION_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::TRANSITION_STARTED),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_TRANSITION_STARTED),
    config_fields: no_config_fields,
    variables: families::transition_variables,
};

#[cfg(test)]
mod tests {
    use super::super::lifecycle;
    use super::super::transition_started::TRANSITION_STARTED;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    const ALL_TRANSITION_KINDS: [&str; 3] = [
        "obs.transition.started",
        "obs.transition.ended",
        "obs.transition.video_ended",
    ];

    fn transition_event(kind: &str, name: &str) -> Event {
        Event::new(EventSource::Obs, kind, json!({ "transition_name": name }))
    }

    #[test]
    fn each_transition_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 3] = [
            ("obs.transition.started", &TRANSITION_STARTED),
            ("obs.transition.ended", &lifecycle::TRANSITION_ENDED),
            (
                "obs.transition.video_ended",
                &lifecycle::TRANSITION_VIDEO_ENDED,
            ),
        ];
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_TRANSITION_KINDS {
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &transition_event(kind, "Fade")),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
        }
    }

    #[test]
    fn transition_descriptor_rejects_non_transition_kind() {
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!TRANSITION_STARTED.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_extracts_transition_name() {
        let stack = TRANSITION_STARTED
            .build_arg_stack(&transition_event("obs.transition.started", "Stinger"));
        assert_eq!(
            stack.get("obs.transition.name"),
            Some(&Variant::String("Stinger".to_owned())),
        );
    }

    #[test]
    fn build_arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "obs.transition.ended", json!({}));
        let stack = TRANSITION_STARTED.build_arg_stack(&event);
        assert!(stack.get("obs.transition.name").is_none());
    }
}
