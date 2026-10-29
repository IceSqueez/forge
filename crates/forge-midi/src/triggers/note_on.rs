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
