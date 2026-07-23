use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::expression as fields;

pub struct ExpressionStateChangedDescriptor;

impl TriggerKindDescriptor for ExpressionStateChangedDescriptor {
    fn id(&self) -> &str {
        "vtube.expression.state_changed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio expression state changed"
    }

    fn summary(&self) -> &str {
        "Fires when a VTube Studio expression is activated or deactivated."
    }

    fn search_text(&self) -> &str {
        "vtube expression state changed activated deactivated"
    }

    fn icon_name(&self) -> &str {
        "smile"
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
        "any expression".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some("vtube.expression.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "vtube.expression.state_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(file) = event
            .payload
            .get(fields::EXPRESSION_FILE)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "vtube.expression.file".to_owned(),
                Variant::String(file.to_owned()),
            );
        }
        if let Some(is_active) = event
            .payload
            .get(fields::IS_ACTIVE)
            .and_then(|v| v.as_bool())
        {
            stack = stack.set(
                "vtube.expression.is_active".to_owned(),
                Variant::Bool(is_active),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "vtube.expression.file".to_owned(),
                    kind: VariantKind::String,
                    label: "Expression file".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "vtube.expression.is_active".to_owned(),
                    kind: VariantKind::Bool,
                    label: "Expression active".to_owned(),
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
    fn matches_only_the_exact_state_changed_kind() {
        let d = ExpressionStateChangedDescriptor;
        let cfg = TriggerConfig::new();
        assert!(d.matches_trigger(&cfg, &event("vtube.expression.state_changed", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.expression.other", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("vtube.hotkey.triggered", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_file_and_is_active_distinguishing_false_from_absent() {
        let d = ExpressionStateChangedDescriptor;
        for (payload, file, active) in [
            (
                json!({ "expression_file": "Smile.exp3.json", "is_active": true }),
                Some("Smile.exp3.json"),
                Some(true),
            ),
            (json!({ "is_active": false }), None, Some(false)),
            (json!({}), None, None),
        ] {
            let stack = d.build_arg_stack(&event("vtube.expression.state_changed", payload));
            assert_eq!(
                stack.get("vtube.expression.file"),
                file.map(|f| Variant::String(f.to_owned())).as_ref()
            );
            assert_eq!(
                stack.get("vtube.expression.is_active"),
                active.map(Variant::Bool).as_ref()
            );
        }
    }
}
