use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, VariableSchema};

pub struct ConnectionConnectedDescriptor;

impl TriggerKindDescriptor for ConnectionConnectedDescriptor {
    fn id(&self) -> &str {
        "obs.connection.connected"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Obs
    }

    fn label(&self) -> &str {
        "OBS connected"
    }

    fn summary(&self) -> &str {
        "Fires when the OBS WebSocket connection is successfully established."
    }

    fn search_text(&self) -> &str {
        "obs connection connected established online"
    }

    fn icon_name(&self) -> &str {
        "plug-connected"
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
        "connected".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Obs),
            kind_prefix: Some("obs.connection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "obs.connection.connected"
    }

    fn build_arg_stack(&self, _event: &Event) -> ArgStack {
        ArgStack::new()
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema { variables: vec![] })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_only_connected_among_connection_lifecycle() {
        let d = ConnectionConnectedDescriptor;
        for (kind, expected) in [
            ("obs.connection.connected", true),
            ("obs.connection.disconnected", false),
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
    fn does_not_match_foreign_kind() {
        let d = ConnectionConnectedDescriptor;
        let event = Event::new(EventSource::Obs, "obs.scene.changed", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &event));
    }
}
