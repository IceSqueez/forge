use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

pub struct MidiProgramChangeDescriptor;

impl TriggerKindDescriptor for MidiProgramChangeDescriptor {
    fn id(&self) -> &str {
        "midi.input.program_change"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Midi
    }

    fn label(&self) -> &str {
        "MIDI Program Change"
    }

    fn summary(&self) -> &str {
        "Fires when a MIDI Program Change message is received."
    }

    fn search_text(&self) -> &str {
        "midi program change patch preset channel"
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
                key: "program",
                label: "Program (leave empty for any)",
                inner: Box::new(FormField::Integer {
                    key: "program",
                    label: "Program",
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
        let program = config.get("program").and_then(|v| {
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
        match (program, channel) {
            (Some(p), Some(ch)) => format!("program={p}, ch={ch}"),
            (Some(p), None) => format!("program={p}"),
            (None, Some(ch)) => format!("ch={ch}"),
            (None, None) => "any program, any channel".to_owned(),
        }
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: Some(EventSource::Midi),
            kind_prefix: Some("midi.input.".to_owned()),
        }
    }

    fn matches_trigger(&self, config: &TriggerConfig, event: &Event) -> bool {
        if event.kind != "midi.input.program_change" {
            return false;
        }
        if let Some(Variant::Int(p)) = config.get("program") {
            let event_program = event
                .payload
                .get("program")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX);
            if *p as u64 != event_program {
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
        if let Some(p) = event.payload.get("program").and_then(|v| v.as_u64()) {
            stack = stack.set("midi.program".to_owned(), Variant::Int(p as i64));
        }
        if let Some(c) = event.payload.get("channel").and_then(|v| v.as_u64()) {
            stack = stack.set("midi.channel".to_owned(), Variant::Int(c as i64));
        }
        if let Some(p) = event.payload.get("port").and_then(|v| v.as_str()) {
            stack = stack.set("midi.port".to_owned(), Variant::String(p.to_owned()));
        }
        stack
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn program_change_event(program: u8, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.program_change",
            json!({ "program": program, "channel": channel, "port": "Synth" }),
        )
    }

    #[test]
    fn matches_program_change_kind() {
        let d = MidiProgramChangeDescriptor;
        let ev = program_change_event(10, 0);
        assert!(d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_sibling_midi_kind() {
        let d = MidiProgramChangeDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.input.note_on",
            json!({ "note": 60, "velocity": 100, "channel": 0, "port": "Synth" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_foreign_event() {
        let d = MidiProgramChangeDescriptor;
        let ev = Event::new(EventSource::Core, "core.tick", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn program_filter_rejects_non_matching_program() {
        let d = MidiProgramChangeDescriptor;
        let ev = program_change_event(20, 0);
        let cfg = BTreeMap::from([("program".to_owned(), Variant::Int(10))]);
        assert!(!d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn program_filter_accepts_matching_program() {
        let d = MidiProgramChangeDescriptor;
        let ev = program_change_event(10, 0);
        let cfg = BTreeMap::from([("program".to_owned(), Variant::Int(10))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn channel_filter_rejects_non_matching_channel() {
        let d = MidiProgramChangeDescriptor;
        let ev = program_change_event(10, 4);
        let cfg = BTreeMap::from([("channel".to_owned(), Variant::Int(2))]);
        assert!(!d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn channel_filter_accepts_matching_channel() {
        let d = MidiProgramChangeDescriptor;
        let ev = program_change_event(10, 2);
        let cfg = BTreeMap::from([("channel".to_owned(), Variant::Int(2))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn build_arg_stack_populates_program_channel_port() {
        let d = MidiProgramChangeDescriptor;
        let ev = program_change_event(42, 7);
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.program"), Some(&Variant::Int(42)));
        assert_eq!(stack.get("midi.channel"), Some(&Variant::Int(7)));
        assert_eq!(
            stack.get("midi.port"),
            Some(&Variant::String("Synth".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_program() {
        let d = MidiProgramChangeDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.input.program_change",
            json!({ "channel": 0, "port": "Synth" }),
        );
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.program"), None);
    }
}
