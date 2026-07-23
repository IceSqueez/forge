use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::connection as fields;

pub struct ConnectionDisconnectedDescriptor;

impl TriggerKindDescriptor for ConnectionDisconnectedDescriptor {
    fn id(&self) -> &str {
        "obs.connection.disconnected"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS disconnected"
    }

    fn summary(&self) -> &str {
        "Fires when the OBS WebSocket connection is lost."
    }

    fn search_text(&self) -> &str {
        "obs connection disconnected lost dropped offline"
    }

    fn icon_name(&self) -> &str {
        "plug-x"
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
        "disconnected".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.connection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.connection.disconnected"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(reason) = event.payload.get(fields::REASON).and_then(|v| v.as_str()) {
            stack = stack.set("obs.reason".to_owned(), Variant::String(reason.to_owned()));
        }
        if let Some(detail) = event.payload.get(fields::DETAIL).and_then(|v| v.as_str()) {
            stack = stack.set("obs.detail".to_owned(), Variant::String(detail.to_owned()));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "obs.reason".to_owned(),
                    kind: VariantKind::String,
                    label: "Disconnect reason".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "obs.detail".to_owned(),
                    kind: VariantKind::String,
                    label: "Disconnect detail".to_owned(),
                    synthesis: None,
                },
            ],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_only_disconnected_among_connection_lifecycle() {
        let d = ConnectionDisconnectedDescriptor;
        for (kind, expected) in [
            ("obs.connection.disconnected", true),
            ("obs.connection.connected", false),
            ("obs.connection.auth_failed", false),
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
    fn build_arg_stack_extracts_reason() {
        let d = ConnectionDisconnectedDescriptor;
        let event = Event::new(
            EventSource::Obs,
            "obs.connection.disconnected",
            json!({ "reason": "connection reset by peer" }),
        );
        assert_eq!(
            d.build_arg_stack(&event).get("obs.reason"),
            Some(&Variant::String("connection reset by peer".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_reason_when_missing() {
        let d = ConnectionDisconnectedDescriptor;
        let event = Event::new(EventSource::Obs, "obs.connection.disconnected", json!({}));
        assert_eq!(d.build_arg_stack(&event).get("obs.reason"), None);
    }
}
