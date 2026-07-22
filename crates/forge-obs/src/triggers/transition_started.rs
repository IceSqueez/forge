use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct TransitionStartedDescriptor;

impl TriggerKindDescriptor for TransitionStartedDescriptor {
    fn id(&self) -> &str {
        "obs.transition.started"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS scene transition started"
    }

    fn summary(&self) -> &str {
        "Fires when a scene transition begins in OBS."
    }

    fn search_text(&self) -> &str {
        "obs scene transition started begin fade cut stinger"
    }

    fn icon_name(&self) -> &str {
        "arrow-right"
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
        "any transition started".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("transition.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "transition.started"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_transition_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: transition_variables(),
        })
    }
}

pub(crate) fn transition_variables() -> Vec<DeclaredVariable> {
    vec![DeclaredVariable {
        name: "obs.transition.name".to_owned(),
        kind: VariantKind::String,
        label: "Transition name".to_owned(),
        synthesis: None,
    }]
}

pub(crate) fn build_transition_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(name) = event
        .payload
        .get("transition_name")
        .and_then(|v| v.as_str())
    {
        stack = stack.set(
            "obs.transition.name".to_owned(),
            Variant::String(name.to_owned()),
        );
    }
    stack
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{
        TransitionEndedDescriptor, TransitionStartedDescriptor, TransitionVideoEndedDescriptor,
    };
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    const ALL_TRANSITION_KINDS: [&str; 3] = [
        "transition.started",
        "transition.ended",
        "transition.video_ended",
    ];

    fn transition_event(kind: &str, name: &str) -> Event {
        Event::new(EventSource::Obs, kind, json!({ "transition_name": name }))
    }

    #[test]
    fn each_transition_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 3] = [
            ("transition.started", &TransitionStartedDescriptor),
            ("transition.ended", &TransitionEndedDescriptor),
            ("transition.video_ended", &TransitionVideoEndedDescriptor),
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
        let event = Event::new(EventSource::Obs, "scene.changed", json!({}));
        assert!(!TransitionStartedDescriptor.matches_trigger(&BTreeMap::new(), &event));
    }

    #[test]
    fn build_arg_stack_extracts_transition_name() {
        let stack = build_transition_arg_stack(&transition_event("transition.started", "Stinger"));
        assert_eq!(
            stack.get("obs.transition.name"),
            Some(&Variant::String("Stinger".to_owned())),
        );
    }

    #[test]
    fn build_arg_stack_omits_name_when_payload_field_absent() {
        let event = Event::new(EventSource::Obs, "transition.ended", json!({}));
        let stack = build_transition_arg_stack(&event);
        assert!(stack.get("obs.transition.name").is_none());
    }
}
