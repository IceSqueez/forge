use serde::{Deserialize, Serialize};

use crate::vocabulary::MidiSignalKind;
use crate::vocabulary::fields::port::direction;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum MidiEvent {
    NoteOn {
        note: u8,
        velocity: u8,
        channel: u8,
    },
    NoteOff {
        note: u8,
        velocity: u8,
        channel: u8,
    },
    ControlChange {
        controller: u8,
        value: u8,
        channel: u8,
    },
    PitchBend {
        value: u16,
        channel: u8,
    },
    ProgramChange {
        program: u8,
        channel: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiOutMessage {
    NoteOn {
        note: u8,
        velocity: u8,
        channel: u8,
    },
    NoteOff {
        note: u8,
        velocity: u8,
        channel: u8,
    },
    ControlChange {
        controller: u8,
        value: u8,
        channel: u8,
    },
    Raw(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct MidiPortInfo {
    pub name: String,
    pub direction: PortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortDirection {
    Input,
    Output,
}

impl PortDirection {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Input => direction::INPUT,
            Self::Output => direction::OUTPUT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiMonitorEvent {
    pub kind: MidiSignalKind,
    pub port_name: String,
    pub channel: u8,
    pub number: Option<u8>,
    pub value: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_direction_serde_roundtrip() {
        let d = PortDirection::Input;
        let json = serde_json::to_string(&d).unwrap();
        let back: PortDirection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn midi_port_info_serde_roundtrip() {
        let p = MidiPortInfo {
            name: "Piano".to_owned(),
            direction: PortDirection::Input,
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: MidiPortInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}
