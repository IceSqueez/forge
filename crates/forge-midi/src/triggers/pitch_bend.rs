use std::collections::BTreeMap;

use forge_events::{Event, EventSource};
use forge_registry::{
    EventFilter, FormField, KindPlatformContract, TriggerCategory, TriggerKindDescriptor,
};
use forge_types::{ArgStack, TriggerConfig, Variant};

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
        vec![FormField::Optional {
            key: "channel",
            label: "Channel (leave empty for any)",
            inner: Box::new(FormField::Integer {
                key: "channel",
                label: "Channel",
                min: 0,
                max: 15,
            }),
        }]
    }

    fn condition_display(&self, config: &TriggerConfig) -> String {
        let channel = config.get("channel").and_then(|v| {
            if let Variant::Int(c) = v {
                Some(*c)
            } else {
                None
            }
        });
        match channel {
            Some(ch) => format!("ch={ch}"),
            None => "any channel".to_owned(),
        }
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
}
