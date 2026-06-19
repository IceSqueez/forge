use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig};

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
            kind_prefix: Some("connection.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "connection.connected"
    }

    fn build_arg_stack(&self, _event: &Event) -> ArgStack {
        ArgStack::new()
    }
}
