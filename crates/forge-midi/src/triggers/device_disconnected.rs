use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct MidiDeviceDisconnectedDescriptor;

impl TriggerKindDescriptor for MidiDeviceDisconnectedDescriptor {
    fn id(&self) -> &str {
        "midi.device.disconnected"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Midi
    }

    fn label(&self) -> &str {
        "MIDI Device Disconnected"
    }

    fn summary(&self) -> &str {
        "Fires when a MIDI port disappears (device unplugged)."
    }

    fn search_text(&self) -> &str {
        "midi device disconnected port removed unplugged input output hotplug"
    }

    fn icon_name(&self) -> &str {
        "music"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Optional {
            key: "direction",
            label: "Direction (leave empty for any)",
            inner: Box::new(FormField::Select {
                key: "direction",
                label: "Direction",
                options: &["input", "output"],
            }),
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        match config.get("direction") {
            Some(Variant::String(d)) if !d.is_empty() => format!("direction={d}"),
            _ => "any direction".to_owned(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Midi),
            kind_prefix: Some("midi.port.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "midi.port.removed" {
            return false;
        }
        if let Some(Variant::String(d)) = config.get("direction")
            && !d.is_empty()
        {
            let event_dir = event.payload.get("direction").and_then(|v| v.as_str());
            if event_dir != Some(d.as_str()) {
                return false;
            }
        }
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event.payload.get("name").and_then(|v| v.as_str()) {
            stack = stack.set(
                "midi.device.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(dir) = event.payload.get("direction").and_then(|v| v.as_str()) {
            stack = stack.set(
                "midi.device.direction".to_owned(),
                Variant::String(dir.to_owned()),
            );
        }
        stack
    }
}
