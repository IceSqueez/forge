use super::input::{MidiInputTrigger, SignalNumber};
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{MidiSignalKind, config_keys, trigger_ids};

pub(crate) const PROGRAM_CHANGE_TRIGGER: MidiInputTrigger = MidiInputTrigger {
    id: trigger_ids::PROGRAM_CHANGE,
    kind: MidiSignalKind::ProgramChange,
    label: "MIDI Program Change",
    summary: "Fires when a MIDI Program Change message is received.",
    search_text: "midi program change patch preset channel",
    icon: "sliders",
    number: Some(SignalNumber {
        config_key: config_keys::PROGRAM,
        optional_label: "Program (leave empty for any)",
        label: "Program",
        display_name: "program",
        any_display: "any program",
        variable: variables::PROGRAM,
        variable_label: "Program number",
    }),
    value: None,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn program_change_event(program: u8, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.program_change",
            json!({ "program": program, "channel": channel, "port_name": "Synth" }),
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
            json!({ "note": 60, "velocity": 100, "channel": 0, "port_name": "Synth" }),
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
            json!({ "channel": 0, "port_name": "Synth" }),
        );
        let stack = d.build_arg_stack(&ev);
        assert_eq!(stack.get("midi.program"), None);
    }
}
