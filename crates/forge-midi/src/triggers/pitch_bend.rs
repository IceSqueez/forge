use super::input::{MidiInputTrigger, SignalValue};
use crate::vocabulary::limits::PITCH_BEND_MAX;
use crate::vocabulary::variables::input as variables;
use crate::vocabulary::{MidiSignalKind, trigger_ids};

pub(crate) const PITCH_BEND_TRIGGER: MidiInputTrigger = MidiInputTrigger {
    id: trigger_ids::PITCH_BEND,
    kind: MidiSignalKind::PitchBend,
    label: "MIDI Pitch Bend",
    summary: "Fires when a MIDI Pitch Bend message is received.",
    search_text: "midi pitch bend wheel channel",
    icon: "sliders",
    number: None,
    value: Some(SignalValue {
        variable: variables::VALUE,
        label: "Pitch bend value",
        max: PITCH_BEND_MAX,
    }),
};

#[cfg(test)]
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
