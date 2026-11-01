use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const FACE_LOST: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::TRACKING_FACE_LOST,
    source: EventSource::VTube,
    label: "VTube Studio face lost",
    summary: "Fires when VTube Studio stops tracking a face.",
    search_text: "vtube tracking face lost out of frame webcam",
    icon: "eye-off",
    prefix: event_kinds::TRACKING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::TRACKING_FACE_LOST),
    condition: TriggerCondition::Fixed(messages::CONDITION_FACE_LOST),
    config_fields: no_config_fields,
    variables: families::tracking_variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::Event;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::{TriggerConfig, Variant};
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> Event {
        Event::new(EventSource::VTube, kind, payload)
    }

    #[test]
    fn matches_only_the_exact_face_lost_kind() {
        let d = FACE_LOST;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.tracking.face_lost", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.tracking.face_found", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.model.loaded", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_hand_bools_distinguishing_false_from_absent() {
        let d = FACE_LOST;
        for (payload, left, right) in [
            (
                json!({ "is_left_hand_found": true, "is_right_hand_found": false }),
                Some(true),
                Some(false),
            ),
            (
                json!({ "is_left_hand_found": false, "is_right_hand_found": false }),
                Some(false),
                Some(false),
            ),
            (json!({ "is_right_hand_found": true }), None, Some(true)),
        ] {
            let stack = d.build_arg_stack(&event("vtube.tracking.face_lost", payload));
            assert_eq!(
                stack.get("vtube.tracking.is_left_hand_found"),
                left.map(Variant::Bool).as_ref()
            );
            assert_eq!(
                stack.get("vtube.tracking.is_right_hand_found"),
                right.map(Variant::Bool).as_ref()
            );
        }
    }
}
