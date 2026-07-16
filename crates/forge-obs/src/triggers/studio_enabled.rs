use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

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
            kind_prefix: Some("studio.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "studio.enabled"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_studio_arg_stack(event)
    }
}

pub(crate) fn build_studio_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(b) = event.payload.get("enabled").and_then(|v| v.as_bool()) {
        stack = stack.set("obs.studio.enabled".to_owned(), Variant::Bool(b));
    }
    stack
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    // Studio Mode enable/disable share one `TriggerKindDescriptor` shape and the
    // `build_studio_arg_stack` helper, so their discrimination contract lives here
    // together rather than re-stating each descriptor's `id()` literal.
    use super::super::{StudioDisabledDescriptor, StudioEnabledDescriptor};
    use super::*;
    use forge_registry::TriggerKindDescriptor;
    use serde_json::json;

    fn studio_event(kind: &str, enabled: bool) -> Event {
        Event::new(EventSource::Obs, kind, json!({ "enabled": enabled }))
    }

    /// Each studio descriptor must fire on exactly its own kind and reject its
    /// sibling - a descriptor that matched the wrong kind would mis-fire actions
    /// (run "studio enabled" handlers when studio mode was turned off).
    #[test]
    fn each_studio_descriptor_matches_only_its_own_kind() {
        let cfg = BTreeMap::new();
        let descriptors: [(&str, &dyn TriggerKindDescriptor); 2] = [
            ("studio.enabled", &StudioEnabledDescriptor),
            ("studio.disabled", &StudioDisabledDescriptor),
        ];
        let kinds = ["studio.enabled", "studio.disabled"];
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
        let event = Event::new(EventSource::Obs, "scene.changed", json!({}));
        let cfg = BTreeMap::new();
        assert!(!StudioEnabledDescriptor.matches_trigger(&cfg, &event));
        assert!(!StudioDisabledDescriptor.matches_trigger(&cfg, &event));
    }

    #[test]
    fn build_arg_stack_extracts_enabled_flag() {
        let stack = build_studio_arg_stack(&studio_event("studio.enabled", true));
        assert_eq!(stack.get("obs.studio.enabled"), Some(&Variant::Bool(true)));
    }

    #[test]
    fn build_arg_stack_omits_key_when_enabled_field_absent() {
        let event = Event::new(EventSource::Obs, "studio.disabled", json!({}));
        let stack = build_studio_arg_stack(&event);
        assert!(stack.get("obs.studio.enabled").is_none());
    }
}
