use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct StudioEnabledDescriptor;

impl TriggerKindDescriptor for StudioEnabledDescriptor {
    fn id(&self) -> &str {
        "obs.studio.enabled"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS Studio Mode enabled"
    }

    fn summary(&self) -> &str {
        "Fires when OBS Studio Mode is turned on."
    }

    fn search_text(&self) -> &str {
        "obs studio mode enabled on preview program"
    }

    fn icon_name(&self) -> &str {
        "layout-columns"
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
        "studio mode enabled".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.studio.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.studio.enabled"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_studio_arg_stack(event)
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: studio_variables(),
        })
    }
}

pub(crate) fn studio_variables() -> Vec<DeclaredVariable> {
    vec![DeclaredVariable {
        name: "obs.studio.enabled".to_owned(),
        kind: VariantKind::Bool,
        label: "Studio mode enabled".to_owned(),
        synthesis: None,
    }]
}

pub(crate) fn build_studio_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    stack = stack.set(
        "obs.studio.enabled".to_owned(),
        Variant::Bool(event.kind == "obs.studio.enabled"),
    );
    stack
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::{StudioDisabledDescriptor, StudioEnabledDescriptor};
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    fn studio_event(kind: &str, enabled: bool) -> Event {
        Event::new(EventSource::Obs, kind, json!({ "enabled": enabled }))
    }

    #[test]
    fn each_studio_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 2] = [
            ("obs.studio.enabled", &StudioEnabledDescriptor),
            ("obs.studio.disabled", &StudioDisabledDescriptor),
        ];
        let kinds = ["obs.studio.enabled", "obs.studio.disabled"];
        for (own_kind, descriptor) in descriptors {
            for kind in kinds {
                assert_eq!(
                    descriptor.matches_trigger(&cfg, &studio_event(kind, true)),
                    kind == own_kind,
                    "descriptor for {own_kind} given {kind}",
                );
            }
        }
    }

    #[test]
    fn studio_descriptor_rejects_non_studio_kind() {
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        let cfg = BTreeMap::new();
        assert!(!StudioEnabledDescriptor.matches_trigger(&cfg, &event));
        assert!(!StudioDisabledDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn arg_stack_derives_enabled_flag_from_kind_with_empty_payload() {
        for (kind, expected) in [("obs.studio.enabled", true), ("obs.studio.disabled", false)] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            let stack = build_studio_arg_stack(&event);
            assert_eq!(
                stack.get("obs.studio.enabled"),
                Some(&Variant::Bool(expected)),
                "kind {kind}",
            );
        }
    }
}
