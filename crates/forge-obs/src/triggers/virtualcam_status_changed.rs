use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct VirtualcamStatusChangedDescriptor;

impl TriggerKindDescriptor for VirtualcamStatusChangedDescriptor {
    fn id(&self) -> &str {
        "obs.virtualcam.state_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS virtual camera state changed"
    }

    fn summary(&self) -> &str {
        "Fires on any virtual camera state transition (starting, started, stopping, stopped)."
    }

    fn search_text(&self) -> &str {
        "obs virtual camera state changed any transition lifecycle"
    }

    fn icon_name(&self) -> &str {
        "camera"
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
        "any virtual camera state change".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("virtualcam.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind.starts_with("virtualcam.")
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        build_virtualcam_arg_stack(event)
    }
}

pub(crate) fn build_virtualcam_arg_stack(event: &Event) -> ArgStack {
    let mut stack = ArgStack::new();
    if let Some(s) = event.payload.get("output_state").and_then(|v| v.as_str()) {
        stack = stack.set(
            "obs.virtualcam.output_state".to_owned(),
            Variant::String(s.to_owned()),
        );
    }
    if let Some(b) = event.payload.get("is_active").and_then(|v| v.as_bool()) {
        stack = stack.set("obs.virtualcam.is_active".to_owned(), Variant::Bool(b));
    }
    stack
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn omnibus_matches_every_virtualcam_lifecycle_kind() {
        let d = VirtualcamStatusChangedDescriptor;
        for kind in [
            "virtualcam.starting",
            "virtualcam.started",
            "virtualcam.stopping",
            "virtualcam.stopped",
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
        let d = VirtualcamStatusChangedDescriptor;
        for kind in ["streaming.started", "recording.stopped", "scene.changed"] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            assert!(
                !d.matches_trigger(&BTreeMap::new(), &event),
                "should reject {kind}"
            );
        }
    }

    #[test]
    fn build_arg_stack_extracts_output_state_and_is_active() {
        let event = Event::new(
            EventSource::Obs,
            "virtualcam.started",
            json!({ "output_state": "started", "is_active": true }),
        );
        let stack = build_virtualcam_arg_stack(&event);
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
            "virtualcam.stopped",
            json!({ "output_state": "stopped", "is_active": false }),
        );
        assert_eq!(
            build_virtualcam_arg_stack(&event).get("obs.virtualcam.is_active"),
            Some(&Variant::Bool(false))
        );
    }

    #[test]
    fn build_arg_stack_omits_keys_when_payload_empty() {
        let event = Event::new(EventSource::Obs, "virtualcam.started", json!({}));
        let stack = build_virtualcam_arg_stack(&event);
        assert_eq!(stack.get("obs.virtualcam.output_state"), None);
        assert_eq!(stack.get("obs.virtualcam.is_active"), None);
    }

    #[test]
    fn build_arg_stack_omits_is_active_when_wrong_json_type() {
        let event = Event::new(
            EventSource::Obs,
            "virtualcam.started",
            json!({ "output_state": "started", "is_active": "true" }),
        );
        let stack = build_virtualcam_arg_stack(&event);
        assert_eq!(
            stack.get("obs.virtualcam.output_state"),
            Some(&Variant::String("started".to_owned()))
        );
        assert_eq!(stack.get("obs.virtualcam.is_active"), None);
    }
}
