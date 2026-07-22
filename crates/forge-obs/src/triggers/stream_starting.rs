use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct StreamStartingDescriptor;

impl TriggerKindDescriptor for StreamStartingDescriptor {
    fn id(&self) -> &str {
        "obs.stream.starting"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS stream starting"
    }

    fn summary(&self) -> &str {
        "Fires when OBS begins the stream start sequence (before output is active)."
    }

    fn search_text(&self) -> &str {
        "obs stream starting go live begin"
    }

    fn icon_name(&self) -> &str {
        "broadcast"
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
        "stream starting".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("streaming.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "streaming.starting"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_stream_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: stream_variables(),
        })
    }
}

pub(crate) fn stream_variables() -> Vec<DeclaredVariable> {
    vec![
        DeclaredVariable {
            name: "obs.stream.output_state".to_owned(),
            kind: VariantKind::String,
            label: "Streaming output state".to_owned(),
            synthesis: None,
        },
        DeclaredVariable {
            name: "obs.stream.is_active".to_owned(),
            kind: VariantKind::Bool,
            label: "Streaming active".to_owned(),
            synthesis: None,
        },
    ]
}

pub(crate) fn build_stream_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(s) = event.payload.get("output_state").and_then(|v| v.as_str()) {
        stack = stack.set(
            "obs.stream.output_state".to_owned(),
            Variant::String(s.to_owned()),
        );
    }
    if let Some(b) = event.payload.get("is_active").and_then(|v| v.as_bool()) {
        stack = stack.set("obs.stream.is_active".to_owned(), Variant::Bool(b));
    }
    stack
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{
        StreamStartedDescriptor, StreamStartingDescriptor, StreamStatusChangedDescriptor,
        StreamStoppedDescriptor, StreamStoppingDescriptor,
    };
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    const ALL_STREAMING_KINDS: [&str; 6] = [
        "streaming.starting",
        "streaming.started",
        "streaming.stopping",
        "streaming.stopped",
        "streaming.reconnecting",
        "streaming.reconnected",
    ];

    fn stream_event(kind: &str, state: &str, active: bool) -> Event {
        Event::new(
            EventSource::Obs,
            kind,
            json!({ "output_state": state, "is_active": active }),
        )
    }

    #[test]
    fn each_specific_descriptor_matches_only_its_own_kind() {
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 4] = [
            ("streaming.starting", &StreamStartingDescriptor),
            ("streaming.started", &StreamStartedDescriptor),
            ("streaming.stopping", &StreamStoppingDescriptor),
            ("streaming.stopped", &StreamStoppedDescriptor),
        ];
        let cfg = BTreeMap::new();
        for (own_kind, descriptor) in descriptors {
            for kind in ALL_STREAMING_KINDS {
                let event = stream_event(kind, "x", true);
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
                StreamStatusChangedDescriptor.matches_trigger(&cfg, &stream_event(kind, "x", true)),
                "omnibus should match {kind}",
            );
        }
    }

    #[test]
    fn omnibus_rejects_non_streaming_kind() {
        let event = Event::new(EventSource::Obs, "scene.changed", json!({}));
        assert!(!StreamStatusChangedDescriptor.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_extracts_output_state_and_is_active() {
        let stack = build_stream_arg_stack(&stream_event("streaming.stopped", "stopped", false));
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
    fn build_arg_stack_omits_keys_when_payload_fields_absent() {
        let event = Event::new(EventSource::Obs, "streaming.started", json!({}));
        let stack = build_stream_arg_stack(&event);
        assert!(stack.get("obs.stream.output_state").is_none());
        assert!(stack.get("obs.stream.is_active").is_none());
    }
}
