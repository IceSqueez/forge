use super::input::{MidiInputTrigger, SignalNumber, SignalValue};
use crate::vocabulary::limits::DATA_BYTE_MAX;
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{MidiSignalKind, config_keys, trigger_ids};

pub(crate) const CONTROL_CHANGE_TRIGGER: MidiInputTrigger = MidiInputTrigger {
    id: trigger_ids::CONTROL_CHANGE,
    kind: MidiSignalKind::ControlChange,
    label: "MIDI Control Change",
    summary: "Fires when a MIDI Control Change (CC) message is received.",
    search_text: "midi cc control change controller knob fader channel",
    icon: "sliders",
    number: Some(SignalNumber {
        config_key: config_keys::CONTROLLER,
        optional_label: "Controller (leave empty for any)",
        label: "Controller",
        display_name: "cc",
        any_display: "any controller",
        variable: variables::CONTROLLER,
        variable_label: "Controller number",
    }),
    value: Some(SignalValue {
        variable: variables::VALUE,
        label: "Controller value",
        max: DATA_BYTE_MAX,
    }),
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use serde_json::json;

    fn cc_event(controller: u8, value: u8, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.control_change",
            json!({ "controller": controller, "value": value, "channel": channel, "port_name": "Pad" }),
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
            json!({ "note": 60, "velocity": 100, "channel": 0, "port_name": "Pad" }),
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
