use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::connection as fields;

pub struct ConnectionAuthFailedDescriptor;

impl TriggerKindDescriptor for ConnectionAuthFailedDescriptor {
    fn id(&self) -> &str {
        "obs.connection.auth_failed"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS authentication failed"
    }

    fn summary(&self) -> &str {
        "Fires when the OBS WebSocket connection is rejected due to authentication failure."
    }

    fn search_text(&self) -> &str {
        "obs connection auth failed password wrong rejected"
    }

    fn icon_name(&self) -> &str {
        "shield-x"
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
        "authentication failed".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("connection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "connection.auth_failed"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(msg) = event
            .payload
            .get(fields::ERROR_MESSAGE)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "obs.error_message".to_owned(),
                Variant::String(msg.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![DeclaredVariable {
                name: "obs.error_message".to_owned(),
                kind: VariantKind::String,
                label: "Error message".to_owned(),
                synthesis: None,
            }],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_only_auth_failed_among_connection_lifecycle() {
        let d = ConnectionAuthFailedDescriptor;
        for (kind, expected) in [
            ("connection.auth_failed", true),
            ("connection.connected", false),
            ("connection.disconnected", false),
        ] {
            let event = Event::new(EventSource::Obs, kind, json!({}));
            assert_eq!(
                d.matches_trigger(&BTreeMap::new(), &event),
                expected,
                "kind {kind}"
            );
        }
    }

    #[test]
    fn build_arg_stack_extracts_error_message() {
        let d = ConnectionAuthFailedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "connection.auth_failed",
            json!({ "error_message": "authentication rejected" }),
        );
        assert_eq!(
            d.build_arg_stack(&event).get("obs.error_message"),
            Some(&Variant::String("authentication rejected".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_error_message_when_missing() {
        let d = ConnectionAuthFailedDescriptor;
        let event = Event::new(EventSource::Obs, "connection.auth_failed", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.error_message"), None);
    }
}
