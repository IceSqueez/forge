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
