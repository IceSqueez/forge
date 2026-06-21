use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

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
}
