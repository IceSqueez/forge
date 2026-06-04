use crate::error::MidiError;
use crate::events::{MidiEvent, MidiOutMessage};

pub(crate) fn decode_midi_bytes(data: &[u8]) -> Result<Option<MidiEvent>, MidiError> {
    if data.is_empty() {
        return Ok(None);
    }
    let status = data[0];
    if status & 0x80 == 0 {
        return Err(MidiError::InvalidStatusByte(status));
    }
    let msg_type = status & 0xF0;
    let channel = status & 0x0F;
    match msg_type {
        0x80 => {
            if data.len() < 3 {
                return Ok(None);
            }
            Ok(Some(MidiEvent::NoteOff {
                note: data[1],
                velocity: data[2],
                channel,
            }))
        }
        0x90 => {
            if data.len() < 3 {
                return Ok(None);
            }
            if data[2] == 0 {
                Ok(Some(MidiEvent::NoteOff {
                    note: data[1],
                    velocity: 0,
                    channel,
                }))
            } else {
                Ok(Some(MidiEvent::NoteOn {
                    note: data[1],
                    velocity: data[2],
                    channel,
                }))
            }
        }
        0xB0 => {
            if data.len() < 3 {
                return Ok(None);
            }
            Ok(Some(MidiEvent::ControlChange {
                controller: data[1],
                value: data[2],
                channel,
            }))
        }
        _ => Ok(None),
    }
}

pub(crate) fn message_to_bytes(message: &MidiOutMessage) -> Result<Vec<u8>, MidiError> {
    match message {
        MidiOutMessage::NoteOn {
            note,
            velocity,
            channel,
        } => Ok(vec![0x90 | channel, *note, *velocity]),
        MidiOutMessage::NoteOff {
            note,
            velocity,
            channel,
        } => Ok(vec![0x80 | channel, *note, *velocity]),
        MidiOutMessage::ControlChange {
            controller,
            value,
            channel,
        } => Ok(vec![0xB0 | channel, *controller, *value]),
        MidiOutMessage::Raw(bytes) => {
            if bytes.is_empty() || (bytes[0] & 0x80 == 0) {
                Err(MidiError::InvalidStatusByte(
                    bytes.first().copied().unwrap_or(0),
                ))
            } else {
                Ok(bytes.clone())
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn note_on_decoded_correctly() {
        let result = decode_midi_bytes(&[0x90, 60, 127]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::NoteOn {
                note: 60,
                velocity: 127,
                channel: 0,
            })
        );
    }

    #[test]
    fn note_on_velocity_zero_becomes_note_off() {
        let result = decode_midi_bytes(&[0x90, 60, 0]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::NoteOff {
                note: 60,
                velocity: 0,
                channel: 0,
            })
        );
    }

    #[test]
    fn note_off_decoded_correctly() {
        let result = decode_midi_bytes(&[0x80, 48, 64]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::NoteOff {
                note: 48,
                velocity: 64,
                channel: 0,
            })
        );
    }

    #[test]
    fn cc_decoded_with_channel() {
        let result = decode_midi_bytes(&[0xB1, 7, 100]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::ControlChange {
                controller: 7,
                value: 100,
                channel: 1,
            })
        );
    }

    #[test]
    fn unsupported_status_byte_returns_none() {
        let result = decode_midi_bytes(&[0xC0, 10]).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn data_byte_without_high_bit_returns_error() {
        let result = decode_midi_bytes(&[0x40, 60, 127]);
        assert!(matches!(result, Err(MidiError::InvalidStatusByte(0x40))));
    }

    #[test]
    fn empty_data_returns_none() {
        let result = decode_midi_bytes(&[]).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn note_on_channel_15() {
        let result = decode_midi_bytes(&[0x9F, 64, 80]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::NoteOn {
                note: 64,
                velocity: 80,
                channel: 15,
            })
        );
    }

    #[test]
    fn cc_channel_2() {
        let result = decode_midi_bytes(&[0xB2, 11, 64]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::ControlChange {
                controller: 11,
                value: 64,
                channel: 2,
            })
        );
    }

    #[test]
    fn message_to_bytes_note_on() {
        let msg = MidiOutMessage::NoteOn {
            note: 72,
            velocity: 64,
            channel: 0,
        };
        assert_eq!(message_to_bytes(&msg).unwrap(), vec![0x90, 72, 64]);
    }

    #[test]
    fn message_to_bytes_note_off() {
        let msg = MidiOutMessage::NoteOff {
            note: 48,
            velocity: 0,
            channel: 1,
        };
        assert_eq!(message_to_bytes(&msg).unwrap(), vec![0x81, 48, 0]);
    }

    #[test]
    fn message_to_bytes_cc() {
        let msg = MidiOutMessage::ControlChange {
            controller: 7,
            value: 127,
            channel: 3,
        };
        assert_eq!(message_to_bytes(&msg).unwrap(), vec![0xB3, 7, 127]);
    }

    #[test]
    fn message_to_bytes_raw_valid() {
        let msg = MidiOutMessage::Raw(vec![0x90, 60, 100]);
        assert_eq!(message_to_bytes(&msg).unwrap(), vec![0x90, 60, 100]);
    }

    #[test]
    fn message_to_bytes_raw_invalid_status_returns_error() {
        let msg = MidiOutMessage::Raw(vec![0x00]);
        assert!(matches!(
            message_to_bytes(&msg),
            Err(MidiError::InvalidStatusByte(0x00))
        ));
    }

    #[test]
    fn message_to_bytes_raw_empty_returns_error() {
        let msg = MidiOutMessage::Raw(vec![]);
        assert!(matches!(
            message_to_bytes(&msg),
            Err(MidiError::InvalidStatusByte(0))
        ));
    }
}
