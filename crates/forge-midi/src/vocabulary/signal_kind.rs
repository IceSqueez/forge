use serde::{Deserialize, Serialize};

use super::event_kinds;
use super::fields::input as fields;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiSignalKind {
    NoteOn,
    NoteOff,
    ControlChange,
    PitchBend,
    ProgramChange,
}

impl MidiSignalKind {
    pub const fn event_kind(self) -> &'static str {
        match self {
            Self::NoteOn => event_kinds::NOTE_ON,
            Self::NoteOff => event_kinds::NOTE_OFF,
            Self::ControlChange => event_kinds::CONTROL_CHANGE,
            Self::PitchBend => event_kinds::PITCH_BEND,
            Self::ProgramChange => event_kinds::PROGRAM_CHANGE,
        }
    }

    pub(crate) const fn number_field(self) -> Option<&'static str> {
        match self {
            Self::NoteOn | Self::NoteOff => Some(fields::NOTE),
            Self::ControlChange => Some(fields::CONTROLLER),
            Self::ProgramChange => Some(fields::PROGRAM),
            Self::PitchBend => None,
        }
    }

    pub(crate) const fn value_field(self) -> Option<&'static str> {
        match self {
            Self::NoteOn | Self::NoteOff => Some(fields::VELOCITY),
            Self::ControlChange | Self::PitchBend => Some(fields::VALUE),
            Self::ProgramChange => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [(MidiSignalKind, &str); 5] = [
        (MidiSignalKind::NoteOn, "note_on"),
        (MidiSignalKind::NoteOff, "note_off"),
        (MidiSignalKind::ControlChange, "control_change"),
        (MidiSignalKind::PitchBend, "pitch_bend"),
        (MidiSignalKind::ProgramChange, "program_change"),
    ];

    #[test]
    fn serde_names_equal_the_previous_monitor_kind_strings() {
        for (kind, name) in ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), serde_json::json!(name));
            let back: MidiSignalKind = serde_json::from_value(serde_json::json!(name)).unwrap();
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn serde_rejects_an_unknown_or_prefixed_kind_name() {
        for bad in ["aftertouch", "NoteOn", "midi.input.note_on", ""] {
            assert!(serde_json::from_value::<MidiSignalKind>(serde_json::json!(bad)).is_err());
        }
    }

    #[test]
    fn event_kind_is_the_input_prefix_plus_the_serde_name() {
        for (kind, name) in ALL {
            assert_eq!(
                kind.event_kind(),
                format!("{}{name}", event_kinds::INPUT_PREFIX)
            );
        }
    }
}
