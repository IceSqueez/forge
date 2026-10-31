use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, UserText, VariableSchema, Variant, VariantKind,
};

use crate::vocabulary::fields::connection as fields;
use crate::vocabulary::messages;
use crate::vocabulary::{event_kinds, trigger_ids, variables};

pub(crate) struct ConnectionDisconnectedDescriptor;

impl TriggerKindDescriptor for ConnectionDisconnectedDescriptor {
    fn id(&self) -> &str {
        trigger_ids::CONNECTION_DISCONNECTED
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

    fn condition_display(&self, _: &TriggerConfig) -> UserText {
        messages::CONDITION_DISCONNECTED.into()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some(event_kinds::CONNECTION_PREFIX.to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == event_kinds::CONNECTION_DISCONNECTED
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(reason) = event.payload.get(fields::REASON).and_then(|v| v.as_str()) {
            stack = stack.set(
                variables::connection::REASON.to_owned(),
                Variant::String(reason.to_owned()),
            );
        }
        if let Some(detail) = event.payload.get(fields::DETAIL).and_then(|v| v.as_str()) {
            stack = stack.set(
                variables::connection::DETAIL.to_owned(),
                Variant::String(detail.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: variables::connection::REASON.to_owned(),
                    kind: VariantKind::String,
                    label: "Disconnect reason".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: variables::connection::DETAIL.to_owned(),
                    kind: VariantKind::String,
                    label: "Disconnect detail".to_owned(),
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
