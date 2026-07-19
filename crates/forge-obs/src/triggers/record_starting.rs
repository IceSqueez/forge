use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct RecordStartingDescriptor;

impl TriggerKindDescriptor for RecordStartingDescriptor {
    fn id(&self) -> &str {
        "obs.record.starting"
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

    fn condition_display(&self, _config: &TriggerConfig) -> String {
        "recording starting".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("recording.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "recording.starting"
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
            name: "obs.record.output_state".to_owned(),
            kind: VariantKind::String,
            label: "Recording output state".to_owned(),
            synthesis: None,
        },
        DeclaredVariable {
            name: "obs.record.is_active".to_owned(),
            kind: VariantKind::Bool,
            label: "Recording active".to_owned(),
            synthesis: None,
        },
        DeclaredVariable {
            name: "obs.record.output_path".to_owned(),
            kind: VariantKind::String,
            label: "Recording file path".to_owned(),
            synthesis: None,
        },
    ]
}

pub(crate) fn build_record_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(s) = event.payload.get("output_state").and_then(|v| v.as_str()) {
        stack = stack.set(
            "obs.record.output_state".to_owned(),
            Variant::String(s.to_owned()),
        );
    }
    if let Some(b) = event.payload.get("is_active").and_then(|v| v.as_bool()) {
        stack = stack.set("obs.record.is_active".to_owned(), Variant::Bool(b));
    }
    if let Some(p) = event.payload.get("output_path").and_then(|v| v.as_str()) {
        stack = stack.set(
            "obs.record.output_path".to_owned(),
            Variant::String(p.to_owned()),
        );
    }
    stack
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // The recording lifecycle descriptors share the `TriggerKindDescriptor` shape and
    // `build_record_arg_stack`, so their discrimination contract is tested together here
    // rather than re-stating each descriptor's `id()` literal.
    use super::super::{
        RecordFileChangedDescriptor, RecordPausedDescriptor, RecordResumedDescriptor,
        RecordStartedDescriptor, RecordStartingDescriptor, RecordStatusChangedDescriptor,
        RecordStoppedDescriptor, RecordStoppingDescriptor,
    };
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    /// The six lifecycle kinds the omnibus descriptor treats as status transitions.
    /// `recording.file_changed` is deliberately NOT here - it is a file-split event,
    /// not a state transition.
    const ALL_LIFECYCLE_KINDS: [&str; 6] = [
        "recording.starting",
        "recording.started",
        "recording.stopping",
        "recording.stopped",
        "recording.paused",
        "recording.resumed",
    ];

    fn record_event(kind: &str) -> Event {
        Event::new(
            EventSource::Obs,
            kind,
            json!({ "output_state": "x", "is_active": true, "output_path": "/tmp/a.mkv" }),
        )
    }

    /// Each lifecycle descriptor must fire on exactly its own kind and reject every
    /// sibling. A descriptor that matched a sibling would mis-fire user actions.
    #[test]
    fn each_specific_descriptor_matches_only_its_own_kind() {
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 6] = [
            ("recording.starting", &RecordStartingDescriptor),
            ("recording.started", &RecordStartedDescriptor),
            ("recording.stopping", &RecordStoppingDescriptor),
            ("recording.stopped", &RecordStoppedDescriptor),
            ("recording.paused", &RecordPausedDescriptor),
            ("recording.resumed", &RecordResumedDescriptor),
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
            // A lifecycle descriptor must also reject the file-split kind.
            assert!(
                !descriptor.matches_trigger(&cfg, &record_event("recording.file_changed")),
                "descriptor for {own_kind} wrongly matched recording.file_changed",
            );
        }
    }

    /// The omnibus descriptor matches all SIX lifecycle kinds.
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

    /// Load-bearing exclusion: `recording.file_changed` is a file-split event, NOT a
    /// lifecycle state transition, so the status-changed omnibus must reject it even
    /// though it shares the `recording.` prefix.
    #[test]
    fn omnibus_rejects_file_changed() {
        assert!(
            !RecordStatusChangedDescriptor
                .matches_trigger(&BTreeMap::new(), &record_event("recording.file_changed")),
        );
    }

    #[test]
    fn omnibus_rejects_non_recording_kind() {
        let event = Event::new(EventSource::Obs, "scene.changed", json!({}));
        assert!(!RecordStatusChangedDescriptor.matches_trigger(&BTreeMap::new(), &event));
    }

    /// The file-changed descriptor is the inverse of the omnibus: it fires on the
    /// file-split kind only and rejects every lifecycle kind.
    #[test]
    fn file_changed_descriptor_matches_only_file_changed() {
        let cfg = BTreeMap::new();
        assert!(
            RecordFileChangedDescriptor
                .matches_trigger(&cfg, &record_event("recording.file_changed")),
        );
        for kind in ALL_LIFECYCLE_KINDS {
            assert!(
                !RecordFileChangedDescriptor.matches_trigger(&cfg, &record_event(kind)),
                "file_changed descriptor wrongly matched {kind}",
            );
        }
    }

    #[test]
    fn build_arg_stack_extracts_state_active_and_path() {
        let event = Event::new(
            EventSource::Obs,
            "recording.started",
            json!({ "output_state": "started", "is_active": true, "output_path": "/rec/a.mkv" }),
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
    fn build_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "recording.started", json!({}));
        let stack = build_record_arg_stack(&event);
        assert!(stack.get("obs.record.output_state").is_none());
        assert!(stack.get("obs.record.is_active").is_none());
        assert!(stack.get("obs.record.output_path").is_none());
    }

    #[test]
    fn file_changed_arg_stack_sets_new_output_path() {
        let event = Event::new(
            EventSource::Obs,
            "recording.file_changed",
            json!({ "output_path_new": "/rec/part2.mkv" }),
        );
        let stack = RecordFileChangedDescriptor.build_arg_stack(&event);
        assert_eq!(
            stack.get("obs.record.output_path_new"),
            Some(&Variant::String("/rec/part2.mkv".to_owned())),
        );
    }

    #[test]
    fn file_changed_arg_stack_omits_path_when_absent() {
        let event = Event::new(EventSource::Obs, "recording.file_changed", json!({}));
        let stack = RecordFileChangedDescriptor.build_arg_stack(&event);
        assert!(stack.get("obs.record.output_path_new").is_none());
    }
}
