use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiEvent {
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
pub struct MidiPortInfo {
    pub name: String,
    pub direction: PortDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    Input,
    Output,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
