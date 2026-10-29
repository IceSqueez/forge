use super::input::MidiInputTrigger;
use super::note_fields::{NOTE, VELOCITY};
use crate::vocabulary::{MidiSignalKind, trigger_ids};

pub(crate) const NOTE_ON_TRIGGER: MidiInputTrigger = MidiInputTrigger {
    id: trigger_ids::NOTE_ON,
    kind: MidiSignalKind::NoteOn,
    label: "MIDI Note On",
    summary: "Fires when a MIDI Note On message is received.",
    search_text: "midi note on press key velocity channel",
    icon: "music",
    number: Some(NOTE),
    value: Some(VELOCITY),
};

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::EventSource;
    use serde_json::json;

    fn note_on_event(note: u8, velocity: u8, channel: u8) -> Event {
        Event::new(
            EventSource::Midi,
            "midi.input.note_on",
            json!({ "note": note, "velocity": velocity, "channel": channel, "port_name": "Piano" }),
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
            json!({ "note": 60, "velocity": 0, "channel": 0, "port_name": "Piano" }),
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

    #[test]
    fn device_filter_admits_only_the_named_port() {
        let d = MidiNoteOnDescriptor;
        let from_piano = note_on_event(60, 100, 0);
        let without_port = Event::new(
            EventSource::Midi,
            "midi.input.note_on",
            json!({ "note": 60, "velocity": 100, "channel": 0 }),
        );
        let string_device =
            |name: &str| BTreeMap::from([("device".to_owned(), Variant::String(name.to_owned()))]);

        let cases: [(&str, TriggerConfig, &Event, bool); 4] = [
            (
                "device equal to the event port",
                string_device("Piano"),
                &from_piano,
                true,
            ),
            (
                "device different from the event port",
                string_device("Keystation"),
                &from_piano,
                false,
            ),
            (
                "non-string device is not a filter",
                BTreeMap::from([("device".to_owned(), Variant::Int(5))]),
                &from_piano,
                true,
            ),
            (
                "event carries no port name",
                string_device("Piano"),
                &without_port,
                false,
            ),
        ];

        for (label, config, event, expected) in cases {
            assert_eq!(d.matches_trigger(&config, event), expected, "{label}");
        }
    }

    #[test]
    fn condition_display_appends_the_device_after_note_and_channel() {
        let d = MidiNoteOnDescriptor;
        let device = || Variant::String("Piano".to_owned());
        let cases: [(TriggerConfig, &str); 4] = [
            (
                BTreeMap::from([("device".to_owned(), device())]),
                "any note, any channel, device=Piano",
            ),
            (
                BTreeMap::from([
                    ("note".to_owned(), Variant::Int(60)),
                    ("device".to_owned(), device()),
                ]),
                "note=60, device=Piano",
            ),
            (
                BTreeMap::from([
                    ("note".to_owned(), Variant::Int(60)),
                    ("channel".to_owned(), Variant::Int(2)),
                    ("device".to_owned(), device()),
                ]),
                "note=60, ch=2, device=Piano",
            ),
            (
                BTreeMap::from([("device".to_owned(), Variant::Int(5))]),
                "any note, any channel",
            ),
        ];

        for (config, expected) in cases {
            assert_eq!(d.condition_display(&config), expected);
        }
    }
}
