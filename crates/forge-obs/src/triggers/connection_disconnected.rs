use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

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
            kind_prefix: Some("connection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "connection.disconnected"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(reason) = event.payload.get("reason").and_then(|v| v.as_str()) {
            stack = stack.set("obs.reason".to_owned(), Variant::String(reason.to_owned()));
        }
        stack
    }
}
