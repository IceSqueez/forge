use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{
    ArgStack, DeclaredVariable, SynthesisHint, TriggerConfig, VariableSchema, Variant, VariantKind,
};

use crate::payload_fields::input as fields;

pub struct MidiPitchBendDescriptor;

impl TriggerKindDescriptor for MidiPitchBendDescriptor {
    fn id(&self) -> &str {
        "midi.input.pitch_bend"
    }

    fn category(&self) -> TriggerCategory {
        TriggerCategory::Midi
    }

    fn label(&self) -> &str {
        "MIDI Pitch Bend"
    }

    fn summary(&self) -> &str {
        "Fires when a MIDI Pitch Bend message is received."
    }

    fn search_text(&self) -> &str {
        "midi pitch bend wheel channel"
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
        let channel = config.get("channel").and_then(|v| {
            if let Variant::Int(c) = v {
                Some(*c)
            } else {
                None
            }
        });
        let device = config.get("device").and_then(|v| {
            if let Variant::String(d) = v
                && !d.is_empty()
            {
                Some(d.clone())
            } else {
                None
            }
        });
        let mut display = match channel {
            Some(ch) => format!("ch={ch}"),
            None => "any channel".to_owned(),
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
        if event.kind != "midi.input.pitch_bend" {
            return false;
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
        if let Some(Variant::String(d)) = config.get("device")
            && !d.is_empty()
        {
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
        if let Some(v) = event.payload.get(fields::VALUE).and_then(|v| v.as_u64()) {
            stack = stack.set("midi.value".to_owned(), Variant::Int(v as i64));
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
                    name: "midi.value".to_owned(),
                    kind: VariantKind::Int,
                    label: "Pitch bend value".to_owned(),
                    synthesis: Some(SynthesisHint::BoundedInt { min: 0, max: 16383 }),
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
    use serde_json::json;

    fn pitch_bend_event(value: u16, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.pitch_bend",
            json!({ "value": value, "channel": channel, "port_name": "Wheel" }),
        )
    }

    #[test]
    fn matches_pitch_bend_kind() {
        let d = MidiPitchBendDescriptor;
        let ev = pitch_bend_event(8192, 0);
        assert!(d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_sibling_midi_kind() {
        let d = MidiPitchBendDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.input.note_on",
            json!({ "note": 60, "velocity": 100, "channel": 0, "port_name": "Wheel" }),
        );
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn does_not_match_foreign_event() {
        let d = MidiPitchBendDescriptor;
        let ev = Event::new(EventSource::Core, "core.tick", json!({}));
        assert!(!d.matches_trigger(&BTreeMap::new(), &ev));
    }

    #[test]
    fn channel_filter_rejects_non_matching_channel() {
        let d = MidiPitchBendDescriptor;
        let ev = pitch_bend_event(8192, 4);
        let cfg = BTreeMap::from([("channel".to_owned(), Variant::Int(2))]);
        assert!(!d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn channel_filter_accepts_matching_channel() {
        let d = MidiPitchBendDescriptor;
        let ev = pitch_bend_event(8192, 2);
        let cfg = BTreeMap::from([("channel".to_owned(), Variant::Int(2))]);
        assert!(d.matches_trigger(&cfg, &ev));
    }

    #[test]
    fn build_arg_stack_populates_value_channel_port() {
        let d = MidiPitchBendDescriptor;
        let ev = pitch_bend_event(16383, 5);
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.value"), Some(&Variant::Int(16383)));
        assert_eq!(stack.get("midi.channel"), Some(&Variant::Int(5)));
        assert_eq!(
            stack.get("midi.port"),
            Some(&Variant::String("Wheel".to_owned()))
        );
    }

    #[test]
    fn build_arg_stack_omits_missing_value() {
        let d = MidiPitchBendDescriptor;
        let ev = Event::new(
            EventSource::Midi,
            "midi.input.pitch_bend",
            json!({ "channel": 0, "port_name": "Wheel" }),
        );
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.value"), None);
    }
}
