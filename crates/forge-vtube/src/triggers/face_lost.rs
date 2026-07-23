use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::tracking as fields;

pub struct FaceLostDescriptor;

impl TriggerKindDescriptor for FaceLostDescriptor {
    fn id(&self) -> &str {
        "vtube.tracking.face_lost"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio face lost"
    }

    fn summary(&self) -> &str {
        "Fires when VTube Studio stops tracking a face."
    }

    fn search_text(&self) -> &str {
        "vtube tracking face lost out of frame webcam"
    }

    fn icon_name(&self) -> &str {
        "eye-off"
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
        "face lost".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some("vtube.tracking.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "vtube.tracking.face_lost"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(left) = event
            .payload
            .get(fields::IS_LEFT_HAND_FOUND)
            .and_then(|v| v.as_bool())
        {
            stack = stack.set(
                "vtube.tracking.is_left_hand_found".to_owned(),
                Variant::Bool(left),
            );
        }
        if let Some(right) = event
            .payload
            .get(fields::IS_RIGHT_HAND_FOUND)
            .and_then(|v| v.as_bool())
        {
            stack = stack.set(
                "vtube.tracking.is_right_hand_found".to_owned(),
                Variant::Bool(right),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "vtube.tracking.is_left_hand_found".to_owned(),
                    kind: VariantKind::Bool,
                    label: "Left hand tracked".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "vtube.tracking.is_right_hand_found".to_owned(),
                    kind: VariantKind::Bool,
                    label: "Right hand tracked".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(kind: &str, payload: serde_json::Value) -> Event {
        Event::new(EventSource::VTube, kind, payload)
    }

    #[test]
    fn matches_only_the_exact_face_lost_kind() {
        let d = FaceLostDescriptor;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.tracking.face_lost", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.tracking.face_found", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.model.loaded", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_hand_bools_distinguishing_false_from_absent() {
        let d = FaceLostDescriptor;
        for (payload, left, right) in [
            (
                json!({ "is_left_hand_found": true, "is_right_hand_found": false }),
                Some(true),
                Some(false),
            ),
            (
                json!({ "is_left_hand_found": false, "is_right_hand_found": false }),
                Some(false),
                Some(false),
            ),
            (json!({ "is_right_hand_found": true }), None, Some(true)),
        ] {
            let stack = d.build_arg_stack(&event("vtube.tracking.face_lost", payload));
            assert_eq!(
                stack.get("vtube.tracking.is_left_hand_found"),
                left.map(Variant::Bool).as_ref()
            );
            assert_eq!(
                stack.get("vtube.tracking.is_right_hand_found"),
                right.map(Variant::Bool).as_ref()
            );
        }
    }
}
