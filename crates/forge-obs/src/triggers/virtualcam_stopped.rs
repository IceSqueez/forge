use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::{event_kinds, messages, trigger_ids};

pub(crate) const VIRTUALCAM_STOPPED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::VIRTUALCAM_STOPPED,
    source: EventSource::Obs,
    label: "OBS virtual camera stopped",
    summary: "Fires when the OBS virtual camera output has fully stopped.",
    search_text: "obs virtual camera stopped off disabled",
    icon: "camera-off",
    prefix: event_kinds::VIRTUALCAM_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::VIRTUALCAM_STOPPED),
    condition: TriggerCondition::Fixed(messages::CONDITION_VIRTUAL_CAMERA_STOPPED),
    config_fields: no_config_fields,
    variables: families::virtualcam_variables,
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn matches_only_stopped_kind_among_virtualcam_lifecycle() {
        let d = VIRTUALCAM_STOPPED;
        for (kind, expected) in [
            ("obs.virtualcam.stopped", true),
            ("obs.virtualcam.started", false),
            ("obs.virtualcam.starting", false),
            ("obs.virtualcam.stopping", false),
        ] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            assert_eq!(
                d.matches_trigger(&BTreeMap::new(), &event),
                expected,
                "kind {kind}"
            );
        }
    }

    #[test]
    fn does_not_match_foreign_source_kind() {
        let d = VIRTUALCAM_STOPPED;
        let event = Event::new(EventSource::Obs, "obs.recording.stopped", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }
}
