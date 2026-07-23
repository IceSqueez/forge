use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::port as fields;

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
            let event_dir = event
                .payload
                .get(fields::DIRECTION)
                .and_then(|v| v.as_str());
            if event_dir != Some(d.as_str()) {
                return false;
            }
        }
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(name) = event
            .payload
            .get(fields::PORT_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "midi.device.name".to_owned(),
                Variant::String(name.to_owned()),
            );
        }
        if let Some(dir) = event
            .payload
            .get(fields::DIRECTION)
            .and_then(|v| v.as_str())
        {
            stack = stack.set(
                "midi.device.direction".to_owned(),
                Variant::String(dir.to_owned()),
            );
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "midi.device.name".to_owned(),
                    kind: VariantKind::String,
                    label: "Device name".to_owned(),
                    synthesis: None,
                },
                DeclaredVariable {
                    name: "midi.device.direction".to_owned(),
                    kind: VariantKind::String,
                    label: "Direction".to_owned(),
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

    fn port_event(kind: &str, name: &str, direction: &str) -> Event {
        Event::new(
            EventSource::Midi,
            kind,
            json!({ "port_name": name, "direction": direction }),
        )
    }

    fn dir_filter(direction: &str) -> TriggerConfig {
        BTreeMap::from([(
            "direction".to_owned(),
            Variant::String(direction.to_owned()),
        )])
    }

    #[test]
    fn matches_port_removed_event() {
        let d = MidiDeviceDisconnectedDescriptor;
        let ev = port_event("midi.port.removed", "Launchpad", "input");
        assert!(d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_port_added_event() {
        let d = MidiDeviceDisconnectedDescriptor;
        let ev = port_event("midi.port.added", "Launchpad", "input");
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_foreign_event_kind() {
        let d = MidiDeviceDisconnectedDescriptor;
        let ev = port_event("midi.input.note_on", "Launchpad", "input");
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn direction_filter_rejects_mismatched_direction() {
        let d = MidiDeviceDisconnectedDescriptor;
        let ev = port_event("midi.port.removed", "Launchpad", "output");
        assert!(!d.matches_trigger(&dir_filter("input"), &ev));
    }

    #[test]
    fn direction_filter_accepts_matching_direction() {
        let d = MidiDeviceDisconnectedDescriptor;
        let ev = port_event("midi.port.removed", "Launchpad", "output");
        assert!(d.matches_trigger(&dir_filter("output"), &ev));
    }

    #[test]
    fn empty_direction_filter_matches_any_direction() {
        let d = MidiDeviceDisconnectedDescriptor;
        let ev = port_event("midi.port.removed", "Launchpad", "input");
        let cfg = BTreeMap::from([("direction".to_owned(), Variant::String(String::new()))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn build_arg_stack_populates_name_and_direction() {
        let d = MidiDeviceDisconnectedDescriptor;
        let ev = port_event("midi.port.removed", "Launchpad", "output");
        let stack = d.build_arg_stack(&ev);
        assert_eq!(
            stack.get("midi.device.name"),
            Some(&Variant::String("Launchpad".to_owned())),
        );
        assert_eq!(
            stack.get("midi.device.direction"),
            Some(&Variant::String("output".to_owned())),
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_direction() {
        let d = MidiDeviceDisconnectedDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.port.removed",
            json!({ "port_name": "Launchpad" }),
        );
        let stack = d.build_arg_stack(&ev);
        assert_eq!(
            stack.get("midi.device.name"),
            Some(&Variant::String("Launchpad".to_owned())),
        );
        assert_eq!(stack.get("midi.device.direction"), None);
    }
}
