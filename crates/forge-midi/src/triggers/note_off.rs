use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, SynthesisHint, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::input as fields;

pub struct MidiNoteOffDescriptor;

impl TriggerKindDescriptor for MidiNoteOffDescriptor {
    fn id(&self) -> &str {
        "midi.input.note_off"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Midi
    }

    fn label(&self) -> &str {
        "MIDI Note Off"
    }

    fn summary(&self) -> &str {
        "Fires when a MIDI Note Off message is received (including velocity-0 Note On)."
    }

    fn search_text(&self) -> &str {
        "midi note off release key channel"
    }

    fn icon_name(&self) -> &str {
        "music-off"
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
                key: "note",
                label: "Note (leave empty for any)",
                inner: Box::new(FormField::Integer {
                    key: "note",
                    label: "Note",
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
            FormField::Optional {
                key: "device",
                label: "Device (leave empty for any)",
                inner: Box::new(FormField::Text {
                    key: "device",
                    label: "Device",
                    placeholder: "e.g. Launchkey Mini",
                }),
            },
        ]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let note = config.get("note").and_then(|v| {
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
        let device = config.get("device").and_then(|v| {
            if let Variant::String(d) = v {
                Some(d.clone())
            } else {
                None
            }
        });
        let mut display = match (note, channel) {
            (Some(n), Some(c)) => format!("note={n}, ch={c}"),
            (Some(n), None) => format!("note={n}"),
            (None, Some(c)) => format!("ch={c}"),
            (None, None) => "any note, any channel".to_owned(),
        };
        if let Some(d) = device {
            display.push_str(&format!(", device={d}"));
        }
        display
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Midi),
            kind_prefix: Some("midi.input.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "midi.input.note_off" {
            return false;
        }
        if let Some(Variant::Int(n)) = config.get("note") {
            let event_note = event
                .payload
                .get(fields::NOTE)
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX);
            if *n as u64 != event_note {
                return false;
            }
        }
        if let Some(Variant::Int(c)) = config.get("channel") {
            let event_ch = event
                .payload
                .get(fields::CHANNEL)
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX);
            if *c as u64 != event_ch {
                return false;
            }
        }
        if let Some(Variant::String(d)) = config.get("device") {
            let event_port = event
                .payload
                .get(fields::PORT_NAME)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if d != event_port {
                return false;
            }
        }
        true
    }

    fn build_arg_stack(&self, event: &Event) -> ArgStack {
        let mut stack = ArgStack::new();
        if let Some(n) = event.payload.get(fields::NOTE).and_then(|v| v.as_u64()) {
            stack = stack.set("midi.note".to_owned(), Variant::Int(n as i64));
        }
        if let Some(v) = event.payload.get(fields::VELOCITY).and_then(|v| v.as_u64()) {
            stack = stack.set("midi.velocity".to_owned(), Variant::Int(v as i64));
        }
        if let Some(c) = event.payload.get(fields::CHANNEL).and_then(|v| v.as_u64()) {
            stack = stack.set("midi.channel".to_owned(), Variant::Int(c as i64));
        }
        if let Some(p) = event
            .payload
            .get(fields::PORT_NAME)
            .and_then(|v| v.as_str())
        {
            stack = stack.set("midi.port".to_owned(), Variant::String(p.to_owned()));
        }
        stack
    }

    fn output_schema(&self) -> Option<VariableSchema> {
        Some(VariableSchema {
            variables: vec![
                DeclaredVariable {
                    name: "midi.note".to_owned(),
                    kind: VariantKind::Int,
                    label: "Note number".to_owned(),
                    synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 127 }),
                },
                DeclaredVariable {
                    name: "midi.velocity".to_owned(),
                    kind: VariantKind::Int,
                    label: "Velocity".to_owned(),
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

    fn note_off_event(note: u8, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.note_off",
            json!({ "note": note, "velocity": 0, "channel": channel, "port": "Piano" }),
        )
    }

    #[test]
    fn matches_any_when_config_empty() {
        let d = MidiNoteOffDescriptor;
        let ev = note_off_event(60, 0);
        assert!(d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn matches_exact_note() {
        let d = MidiNoteOffDescriptor;
        let ev = note_off_event(48, 0);
        let cfg = BTreeMap::from([("note".to_owned(), Variant::Int(48))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn rejects_different_note() {
        let d = MidiNoteOffDescriptor;
        let ev = note_off_event(49, 0);
        let cfg = BTreeMap::from([("note".to_owned(), Variant::Int(48))]);
        assert!(!d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn does_not_match_note_on_event() {
        let d = MidiNoteOffDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.input.note_on",
            json!({ "note": 60, "velocity": 100, "channel": 0, "port": "Piano" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn build_arg_stack_populates_midi_keys() {
        let d = MidiNoteOffDescriptor;
        let ev = note_off_event(60, 2);
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.note"), Some(&Variant::Int(60)));
        assert_eq!(stack.get("midi.channel"), Some(&Variant::Int(2)));
    }
}
