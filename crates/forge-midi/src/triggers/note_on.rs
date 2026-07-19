use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, SynthesisHint, TriggerConfig, VariableSchema, Variant, VariantKind,
};

pub struct MidiNoteOnDescriptor;

impl TriggerKindDescriptor for MidiNoteOnDescriptor {
    fn id(&self) -> &str {
        "midi.input.note_on"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Midi
    }

    fn label(&self) -> &str {
        "MIDI Note On"
    }

    fn summary(&self) -> &str {
        "Fires when a MIDI Note On message is received."
    }

    fn search_text(&self) -> &str {
        "midi note on press key velocity channel"
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
        match (note, channel) {
            (Some(n), Some(c)) => format!("note={n}, ch={c}"),
            (Some(n), None) => format!("note={n}"),
            (None, Some(c)) => format!("ch={c}"),
            (None, None) => "any note, any channel".to_owned(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Midi),
            kind_prefix: Some("midi.input.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "midi.input.note_on" {
            return false;
        }
        if let Some(Variant::Int(n)) = config.get("note") {
            let event_note = event
                .payload
                .get("note")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX);
            if *n as u64 != event_note {
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
        if let Some(n) = event.payload.get("note").and_then(|v| v.as_u64()) {
            stack = stack.set("midi.note".to_owned(), Variant::Int(n as i64));
        }
        if let Some(v) = event.payload.get("velocity").and_then(|v| v.as_u64()) {
            stack = stack.set("midi.velocity".to_owned(), Variant::Int(v as i64));
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

    fn note_on_event(note: u8, velocity: u8, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.note_on",
            json!({ "note": note, "velocity": velocity, "channel": channel, "port": "Piano" }),
        )
    }

    #[test]
    fn matches_any_when_config_empty() {
        let d = MidiNoteOnDescriptor;
        let ev = note_on_event(60, 100, 0);
        assert!(d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn matches_exact_note() {
        let d = MidiNoteOnDescriptor;
        let ev = note_on_event(60, 100, 0);
        let cfg = BTreeMap::from([("note".to_owned(), Variant::Int(60))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn rejects_different_note() {
        let d = MidiNoteOnDescriptor;
        let ev = note_on_event(61, 100, 0);
        let cfg = BTreeMap::from([("note".to_owned(), Variant::Int(60))]);
        assert!(!d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn matches_exact_channel() {
        let d = MidiNoteOnDescriptor;
        let ev = note_on_event(60, 100, 2);
        let cfg = BTreeMap::from([("channel".to_owned(), Variant::Int(2))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn rejects_different_channel() {
        let d = MidiNoteOnDescriptor;
        let ev = note_on_event(60, 100, 3);
        let cfg = BTreeMap::from([("channel".to_owned(), Variant::Int(2))]);
        assert!(!d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn does_not_match_note_off_event() {
        let d = MidiNoteOnDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.input.note_off",
            json!({ "note": 60, "velocity": 0, "channel": 0, "port": "Piano" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn build_arg_stack_populates_midi_keys() {
        let d = MidiNoteOnDescriptor;
        let ev = note_on_event(60, 100, 1);
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.note"), Some(&Variant::Int(60)));
        assert_eq!(stack.get("midi.velocity"), Some(&Variant::Int(100)));
        assert_eq!(stack.get("midi.channel"), Some(&Variant::Int(1)));
        assert_eq!(
            stack.get("midi.port"),
            Some(&Variant::String("Piano".to_owned()))
        );
    }

    #[test]
    fn condition_display_any_when_no_config() {
        let d = MidiNoteOnDescriptor;
        assert_eq!(
            d.condition_display(&BTreeMap::new()),
            "any note, any channel"
        );
    }

    #[test]
    fn condition_display_with_note() {
        let d = MidiNoteOnDescriptor;
        let cfg = BTreeMap::from([("note".to_owned(), Variant::Int(60))]);
        assert_eq!(d.condition_display(&cfg), "note=60");
    }
}
