use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, SynthesisHint, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct MidiCcDescriptor;

impl TriggerKindDescriptor for MidiCcDescriptor {
    fn id(&self) -> &str {
        "midi.input.control_change"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Midi
    }

    fn label(&self) -> &str {
        "MIDI Control Change"
    }

    fn summary(&self) -> &str {
        "Fires when a MIDI Control Change (CC) message is received."
    }

    fn search_text(&self) -> &str {
        "midi cc control change controller knob fader channel"
    }

    fn icon_name(&self) -> &str {
        "sliders"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::Universal
    }

    fn default_config(&self) -> TriggerConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Optional {
                key: "controller",
                label: "Controller (leave empty for any)",
                inner: Box::new(FormField::Integer {
                    key: "controller",
                    label: "Controller",
                    min: 0,
                    max: 127,
                }),
            },
            FormField::Optional {
                key: "channel",
                label: "Channel (leave empty for any)",
                inner: Box::new(FormField::Integer {
                    key: "channel",
                    label: "Channel",
                    min: 0,
                    max: 15,
                }),
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let ctrl = config.get("controller").and_then(|v| {
            if let Variant::Int(n) = v {
                Some(*n)
            } else {
                None
            }
        });
        let channel = config.get("channel").and_then(|v| {
            if let Variant::Int(c) = v {
                Some(*c)
            } else {
                None
            }
        });
        match (ctrl, channel) {
            (Some(c), Some(ch)) => format!("cc={c}, ch={ch}"),
            (Some(c), None) => format!("cc={c}"),
            (None, Some(ch)) => format!("ch={ch}"),
            (None, None) => "any controller, any channel".to_owned(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Midi),
            kind_prefix: Some("midi.input.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "midi.input.control_change" {
            return false;
        }
        if let Some(Variant::Int(c)) = config.get("controller") {
            let event_ctrl = event
                .payload
                .get("controller")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX);
            if *c as u64 != event_ctrl {
                return false;
            }
        }
        if let Some(Variant::Int(c)) = config.get("channel") {
            let event_ch = event
                .payload
                .get("channel")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX);
            if *c as u64 != event_ch {
                return false;
            }
        }
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(c) = event.payload.get("controller").and_then(|v| v.as_u64()) {
            stack = stack.set("midi.controller".to_owned(), Variant::Int(c as i64));
        }
        if let Some(v) = event.payload.get("value").and_then(|v| v.as_u64()) {
            stack = stack.set("midi.value".to_owned(), Variant::Int(v as i64));
        }
        if let Some(c) = event.payload.get("channel").and_then(|v| v.as_u64()) {
            stack = stack.set("midi.channel".to_owned(), Variant::Int(c as i64));
        }
        if let Some(p) = event.payload.get("port").and_then(|v| v.as_str()) {
            stack = stack.set("midi.port".to_owned(), Variant::String(p.to_owned()));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "midi.controller".to_owned(),
                    kind: VariantKind::Int,
                    label: "Controller number".to_owned(),
                    synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 127 }),
                },
                DeclaredVariable {
                    name: "midi.value".to_owned(),
                    kind: VariantKind::Int,
                    label: "Controller value".to_owned(),
                    synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 127 }),
                },
                DeclaredVariable {
                    name: "midi.channel".to_owned(),
                    kind: VariantKind::Int,
                    label: "Channel".to_owned(),
                    synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 15 }),
                },
                DeclaredVariable {
                    name: "midi.port".to_owned(),
                    kind: VariantKind::String,
                    label: "Port name".to_owned(),
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
    use forge_events::EventSource;
    use serde_json::json;

    fn cc_event(controller: u8, value: u8, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.control_change",
            json!({ "controller": controller, "value": value, "channel": channel, "port": "Pad" }),
        )
    }

    #[test]
    fn matches_any_when_config_empty() {
        let d = MidiCcDescriptor;
        let ev = cc_event(7, 100, 0);
        assert!(d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn matches_exact_controller() {
        let d = MidiCcDescriptor;
        let ev = cc_event(7, 100, 0);
        let cfg = BTreeMap::from([("controller".to_owned(), Variant::Int(7))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn rejects_different_controller() {
        let d = MidiCcDescriptor;
        let ev = cc_event(11, 100, 0);
        let cfg = BTreeMap::from([("controller".to_owned(), Variant::Int(7))]);
        assert!(!d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn matches_controller_and_channel() {
        let d = MidiCcDescriptor;
        let ev = cc_event(7, 100, 2);
        let cfg = BTreeMap::from([
            ("controller".to_owned(), Variant::Int(7)),
            ("channel".to_owned(), Variant::Int(2)),
        ]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn does_not_match_note_on_event() {
        let d = MidiCcDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.input.note_on",
            json!({ "note": 60, "velocity": 100, "channel": 0, "port": "Pad" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn build_arg_stack_populates_midi_keys() {
        let d = MidiCcDescriptor;
        let ev = cc_event(7, 64, 1);
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.controller"), Some(&Variant::Int(7)));
        assert_eq!(stack.get("midi.value"), Some(&Variant::Int(64)));
        assert_eq!(stack.get("midi.channel"), Some(&Variant::Int(1)));
    }
}
