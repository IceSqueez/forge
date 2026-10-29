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
