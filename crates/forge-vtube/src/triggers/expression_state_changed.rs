use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

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
            kind_prefix: Some("expression.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "expression.state_changed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get("expression_name")
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "vtube.expression.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(active) = event.payload.get("active").and_then(|v| v.as_bool()) {
            stack = stack.set("vtube.expression.active".to_owned(), Variant::Bool(active));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "vtube.expression.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Expression name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "vtube.expression.active".to_owned(),
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
        assert!(d.matches_trigger(&cfg, &event("expression.state_changed", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("expression.other", json!({}))));
        assert!(!d.matches_trigger(&cfg, &event("hotkey.triggered", json!({}))));
    }

    #[test]
    fn build_arg_stack_maps_name_and_bool_active() {
        let d = ExpressionStateChangedDescriptor;
        let stack = d.build_arg_stack(&event(
            "expression.state_changed",
            json!({ "expression_name": "Smile", "active": true }),
        ));
        assert_eq!(
            stack.get("vtube.expression.name"),
            Some(&Variant::String("Smile".to_owned()))
        );
        assert_eq!(
            stack.get("vtube.expression.active"),
            Some(&Variant::Bool(true))
        );
    }

    #[test]
    fn build_arg_stack_preserves_active_false() {
        let d = ExpressionStateChangedDescriptor;
        let stack = d.build_arg_stack(&event(
            "expression.state_changed",
            json!({ "active": false }),
        ));
        assert_eq!(
            stack.get("vtube.expression.active"),
            Some(&Variant::Bool(false))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_keys() {
        let d = ExpressionStateChangedDescriptor;
        let stack = d.build_arg_stack(&event("expression.state_changed", json!({})));
        assert!(stack.get("vtube.expression.name").is_none());
        assert!(stack.get("vtube.expression.active").is_none());
    }
}
