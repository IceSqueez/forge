use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct HotkeyTriggeredDescriptor;

impl TriggerKindDescriptor for HotkeyTriggeredDescriptor {
    fn id(&self) -> &str {
        "vtube.hotkey.triggered"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::VTube
    }

    fn label(&self) -> &str {
        "VTube Studio hotkey triggered"
    }

    fn summary(&self) -> &str {
        "Fires when a VTube Studio hotkey is activated."
    }

    fn search_text(&self) -> &str {
        "vtube hotkey triggered activated shortcut"
    }

    fn icon_name(&self) -> &str {
        "zap"
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
        "any hotkey".to_owned()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::VTube),
            kind_prefix: Some("hotkey.".to_owned()),
        }
    }

    fn matches_trigger(&self, _config: &TriggerConfig, event: &Event) -> bool {
        event.kind == "hotkey.triggered"
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("hotkey_name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "vtube.hotkey.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(id) = event.payload.get("hotkey_id").and_then(|v| v.as_str()) {
            stack = stack.set("vtube.hotkey.id".to_owned(), Variant::String(id.to_owned()));
        }
        stack
    }
}
