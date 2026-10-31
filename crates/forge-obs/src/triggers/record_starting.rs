use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::recording as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct RecordStartingDescriptor;

impl TriggerKindDescriptor for RecordStartingDescriptor {
    fn id(&self) -> &str {
        trigger_ids::RECORDING_STARTING
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS recording starting"
    }

    fn summary(&self) -> &str {
        "Fires when OBS begins the recording start sequence (before output is active)."
    }

    fn search_text(&self) -> &str {
        "obs recording starting begin capture"
    }

    fn icon_name(&self) -> &str {
        "record"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        messages::CONDITION_RECORDING_STARTING.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::RECORDING_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::RECORDING_STARTING
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_record_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: record_variables(),
        })
    }
}

pub(crate) fn record_variables() -> Vec<DeclaredVariable> {
    vec![
        DeclaredVariable {
            name: variables::recording::OUTPUT_STATE.to_owned(),
            kind: VariantKind::String,
            label: "Recording output state".to_owned(),
            synthesis: None,
        },
        DeclaredVariable {
            name: variables::recording::IS_ACTIVE.to_owned(),
            kind: VariantKind::Bool,
            label: "Recording active".to_owned(),
            synthesis: None,
        },
        DeclaredVariable {
            name: variables::recording::OUTPUT_PATH.to_owned(),
            kind: VariantKind::String,
            label: "Recording file path".to_owned(),
            synthesis: None,
        },
    ]
}

pub(crate) fn build_record_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(s) = event.kind.rsplit('.').next() {
        stack = stack.set(
            variables::recording::OUTPUT_STATE.to_owned(),
            Variant::String(s.to_owned()),
        );
    }
    if let Some(b) = event
        .payload
        .get(fields::IS_ACTIVE)
        .and_then(|v| v.as_bool())
    {
        stack = stack.set(variables::recording::IS_ACTIVE.to_owned(), Variant::Bool(b));
    }
    if let Some(p) = event
        .payload
        .get(fields::OUTPUT_PATH)
        .and_then(|v| v.as_str())
    {
        stack = stack.set(
            variables::recording::OUTPUT_PATH.to_owned(),
            Variant::String(p.to_owned()),
        );
    }
    stack
}

#[cfg(test)]
mod tests {
    use super::super::{
        RecordFileChangedDescriptor, RecordPausedDescriptor, RecordResumedDescriptor,
        RecordStartedDescriptor, RecordStartingDescriptor, RecordStatusChangedDescriptor,
        RecordStoppedDescriptor, RecordStoppingDescriptor,
    };
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

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
            ("obs.recording.starting", &RecordStartingDescriptor),
            ("obs.recording.started", &RecordStartedDescriptor),
            ("obs.recording.stopping", &RecordStoppingDescriptor),
            ("obs.recording.stopped", &RecordStoppedDescriptor),
            ("obs.recording.paused", &RecordPausedDescriptor),
            ("obs.recording.resumed", &RecordResumedDescriptor),
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
                RecordStatusChangedDescriptor.matches_trigger(&cfg, &record_event(kind)),
                "omnibus should match {kind}",
            );
        }
    }

    #[test]
    fn omnibus_rejects_file_changed() {
        assert!(!RecordStatusChangedDescriptor.matches_trigger(
            &BTreeMap::new(),
            &record_event("obs.recording.file_changed")
        ),);
    }

    #[test]
    fn omnibus_rejects_non_recording_kind() {
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!RecordStatusChangedDescriptor.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn file_changed_descriptor_matches_only_file_changed() {
        let cfg = BTreeMap::new();
        assert!(
            RecordFileChangedDescriptor
                .matches_trigger(&cfg, &record_event("obs.recording.file_changed")),
        );
        for kind in ALL_LIFECYCLE_KINDS {
            assert!(
                !RecordFileChangedDescriptor.matches_trigger(&cfg, &record_event(kind)),
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
        let stack = build_record_arg_stack(&event);
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
        let stack = build_record_arg_stack(&event);
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
        let stack = RecordFileChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.record.output_path"),
            Some(&Variant::String("/rec/part2.mkv".to_owned())),
        );
    }

    #[test]
    fn file_changed_arg_stack_omits_path_when_absent() {
        let event = Event::new(EventSource::Obs, "obs.recording.file_changed", json!({}));
        let stack = RecordFileChangedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.record.output_path").is_none());
    }
}
