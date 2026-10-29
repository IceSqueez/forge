use super::input::MidiInputTrigger;
use super::note_fields::{NOTE, VELOCITY};
use crate::vocabulary::{MidiSignalKind, trigger_ids};

pub(crate) const NOTE_OFF_TRIGGER: MidiInputTrigger = MidiInputTrigger {
    id: trigger_ids::NOTE_OFF,
    kind: MidiSignalKind::NoteOff,
    label: "MIDI Note Off",
    summary: "Fires when a MIDI Note Off message is received (including velocity-0 Note On).",
    search_text: "midi note off release key channel",
    icon: "music-off",
    number: Some(NOTE),
    value: Some(VELOCITY),
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use serde_json::json;

    fn note_off_event(note: u8, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.note_off",
            json!({ "note": note, "velocity": 0, "channel": channel, "port_name": "Piano" }),
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
            json!({ "note": 60, "velocity": 100, "channel": 0, "port_name": "Piano" }),
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
