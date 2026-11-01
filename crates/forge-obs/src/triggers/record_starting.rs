use super::families;
use forge_events::EventSource;
use forge_registry::{EventTriggerSpec, TriggerCondition, TriggerMatcher, no_config_fields};

use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids};

pub(crate) const RECORD_STARTING: EventTriggerSpec = EventTriggerSpec {
    id: trigger_ids::RECORDING_STARTING,
    source: EventSource::Obs,
    label: "OBS recording starting",
    summary: "Fires when OBS begins the recording start sequence (before output is active).",
    search_text: "obs recording starting begin capture",
    icon: "record",
    prefix: event_kinds::RECORDING_PREFIX,
    matcher: TriggerMatcher::Kind(event_kinds::RECORDING_STARTING),
    condition: TriggerCondition::Fixed(messages::CONDITION_RECORDING_STARTING),
    config_fields: no_config_fields,
    variables: families::record_variables,
};

#[cfg(test)]
mod tests {
    use super::super::lifecycle;
    use super::super::record_file_changed::RECORD_FILE_CHANGED;
    use super::super::record_starting::RECORD_STARTING;
    use super::super::record_status_changed::RECORD_STATUS_CHANGED;
    use forge_events::{Event, EventSource};
    use forge_registry::TriggerKindDescriptor;
    use forge_types::Variant;
    use serde_json::json;
    use std::collections::BTreeMap;

    const ALL_LIFECYCLE_KINDS: [&str; 6] = [
        "obs.recording.starting",
        "obs.recording.started",
        "obs.recording.stopping",
        "obs.recording.stopped",
        "obs.recording.paused",
        "obs.recording.resumed",
    ];

    fn record_event(kind: &str) -> Event {
        Event::new(
            EventSource::Obs,
            kind,
            json!({ "is_active": true, "output_path": "/tmp/a.mkv" }),
        )
    }

    #[test]
    fn each_specific_descriptor_matches_only_its_own_kind() {
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 6] = [
            ("obs.recording.starting", &RECORD_STARTING),
            ("obs.recording.started", &lifecycle::RECORDING_STARTED),
            ("obs.recording.stopping", &lifecycle::RECORDING_STOPPING),
            ("obs.recording.stopped", &lifecycle::RECORDING_STOPPED),
            ("obs.recording.paused", &lifecycle::RECORDING_PAUSED),
            ("obs.recording.resumed", &lifecycle::RECORDING_RESUMED),
        ];
        let cfg = BTreeMap::new();
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_LIFECYCLE_KINDS {
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &record_event(kind)),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
            assert!(
                !descriptor.matches_trigger(&cfg, &record_event("obs.recording.file_changed")),
                "descriptor for {own_kind} wrongly matched recording.file_changed",
            );
        }
    }

    #[test]
    fn omnibus_matches_every_lifecycle_kind() {
        let cfg = BTreeMap::new();
        for kind in ALL_LIFECYCLE_KINDS {
            assert!(
                RECORD_STATUS_CHANGED.matches_trigger(&cfg, &record_event(kind)),
                "omnibus should match {kind}",
            );
        }
    }

    #[test]
    fn omnibus_rejects_file_changed() {
        assert!(!RECORD_STATUS_CHANGED.matches_trigger(
            &BTreeMap::new(),
            &record_event("obs.recording.file_changed")
        ),);
    }

    #[test]
    fn omnibus_rejects_non_recording_kind() {
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!RECORD_STATUS_CHANGED.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn file_changed_descriptor_matches_only_file_changed() {
        let cfg = BTreeMap::new();
        assert!(
            RECORD_FILE_CHANGED.matches_trigger(&cfg, &record_event("obs.recording.file_changed")),
        );
        for kind in ALL_LIFECYCLE_KINDS {
            assert!(
                !RECORD_FILE_CHANGED.matches_trigger(&cfg, &record_event(kind)),
                "file_changed descriptor wrongly matched {kind}",
            );
        }
    }

    #[test]
    fn build_arg_stack_takes_the_state_from_the_kind_and_the_rest_from_the_payload() {
        let event = Event::new(
            EventSource::Obs,
            "obs.recording.started",
            json!({ "is_active": true, "output_path": "/rec/a.mkv" }),
        );
        let stack = RECORD_STARTING.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.record.output_state"),
            Some(&Variant::String("started".to_owned())),
        );
        assert_eq!(
            stack.get("obs.record.is_active"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(
            stack.get("obs.record.output_path"),
            Some(&Variant::String("/rec/a.mkv".to_owned())),
        );
    }

    #[test]
    fn build_arg_stack_derives_output_state_from_kind_when_payload_empty() {
        let event = Event::new(EventSource::Obs, "obs.recording.started", json!({}));
        let stack = RECORD_STARTING.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.record.output_state"),
            Some(&Variant::String("started".to_owned())),
        );
        assert!(stack.get("obs.record.is_active").is_none());
        assert!(stack.get("obs.record.output_path").is_none());
    }

    #[test]
    fn file_changed_arg_stack_sets_output_path() {
        let event = Event::new(
            EventSource::Obs,
            "obs.recording.file_changed",
            json!({ "output_path": "/rec/part2.mkv" }),
        );
        let stack = RECORD_FILE_CHANGED.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.record.output_path"),
            Some(&Variant::String("/rec/part2.mkv".to_owned())),
        );
    }

    #[test]
    fn file_changed_arg_stack_omits_path_when_absent() {
        let event = Event::new(EventSource::Obs, "obs.recording.file_changed", json!({}));
        let stack = RECORD_FILE_CHANGED.build_arg_stack(&event);
        assert!(stack.get("obs.record.output_path").is_none());
    }
}
