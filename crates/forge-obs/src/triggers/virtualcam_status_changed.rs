use super::families;
use forge_events::Event;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};
use forge_types::TriggerConfig;

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

fn matches(_config: &TriggerConfig, event: &Event) -> bool {
    event.kind.starts_with(event_kinds::VIRTUALCAM_PREFIX)
}

pub(crate) const VIRTUALCAM_STATUS_CHANGED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::VIRTUALCAM_STATUS_CHANGED,
    source: EventSource::Obs,
    label: "OBS virtual camera state changed",
    summary: "Fires on any virtual camera state transition (starting, started, stopping, stopped).",
    search_text: "obs virtual camera state changed any transition lifecycle",
    icon: "camera",
    prefix: event_kinds::VIRTUALCAM_PREFIX,
    matcher: TriggerMatcher::Custom(matches),
    condition: TriggerCondition::Fixed(messages::CONDITION_ANY_VIRTUAL_CAMERA_STATE_CHANGE),
    config_fields: no_config_fields,
    variables: families::virtualcam_variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn omnibus_matches_every_virtualcam_lifecycle_kind() {
        let d = VIRTUALCAM_STATUS_CHANGED;
        for kind in [
            "obs.virtualcam.starting",
            "obs.virtualcam.started",
            "obs.virtualcam.stopping",
            "obs.virtualcam.stopped",
        ] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            assert!(
                d.matches_trigger(&BTreeMap::new(), &event),
                "should match {kind}"
            );
        }
    }

    #[test]
    fn omnibus_rejects_non_virtualcam_kinds() {
        let d = VIRTUALCAM_STATUS_CHANGED;
        for kind in [
            "obs.streaming.started",
            "obs.recording.stopped",
            "obs.scene.changed",
        ] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            assert!(
                !d.matches_trigger(&BTreeMap::new(), &event),
                "should reject {kind}"
            );
        }
    }

    #[test]
    fn build_arg_stack_takes_the_state_from_the_kind_and_is_active_from_the_payload() {
        let event = Event::new(
            EventSource::Obs,
            "obs.virtualcam.started",
            json!({ "is_active": true }),
        );
        let stack = VIRTUALCAM_STATUS_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.virtualcam.output_state"),
            Some(&Variant::String("started".to_owned()))
        );
        assert_eq!(
            stack.get("obs.virtualcam.is_active"),
            Some(&Variant::Bool(true))
        );
    }

    #[test]
    fn build_arg_stack_extracts_is_active_false() {
        let event = Event::new(
            EventSource::Obs,
            "obs.virtualcam.stopped",
            json!({ "is_active": false }),
        );
        assert_eq!(
            VIRTUALCAM_STATUS_CHANGED
                .build_arg_stack(&event)
                .get("obs.virtualcam.is_active"),
            Some(&Variant::Bool(false))
        );
    }

    #[test]
    fn build_arg_stack_derives_output_state_from_kind_when_payload_empty() {
        let event = Event::new(EventSource::Obs, "obs.virtualcam.started", json!({}));
        let stack = VIRTUALCAM_STATUS_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.virtualcam.output_state"),
            Some(&Variant::String("started".to_owned())),
        );
        assert_eq!(stack.get("obs.virtualcam.is_active"), None);
    }

    #[test]
    fn build_arg_stack_omits_is_active_when_wrong_json_type() {
        let event = Event::new(
            EventSource::Obs,
            "obs.virtualcam.started",
            json!({ "is_active": "true" }),
        );
        let stack = VIRTUALCAM_STATUS_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.virtualcam.output_state"),
            Some(&Variant::String("started".to_owned()))
        );
        assert_eq!(stack.get("obs.virtualcam.is_active"), None);
    }
}
