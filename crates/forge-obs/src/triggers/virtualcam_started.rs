use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::{event_kinds, messages, trigger_ids};

pub(crate) const VIRTUALCAM_STARTED: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::VIRTUALCAM_STARTED,
    source: EventSource::Obs,
    label: "OBS virtual camera started",
    summary: "Fires when the OBS virtual camera output becomes active.",
    search_text: "obs virtual camera started active on",
    icon: "camera",
    prefix: event_kinds::VIRTUALCAM_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::VIRTUALCAM_STARTED),
    condition: TriggerCondition::Fixed(messages::CONDITION_VIRTUAL_CAMERA_STARTED),
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
    fn matches_only_started_kind_among_virtualcam_lifecycle() {
        let d = VIRTUALCAM_STARTED;
        for (kind, expected) in [
            ("obs.virtualcam.started", true),
            ("obs.virtualcam.stopped", false),
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
        let d = VIRTUALCAM_STARTED;
        let event = Event::new(EventSource::Obs, "obs.streaming.started", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }
}
