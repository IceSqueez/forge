use crate::error::MidiError;
use crate::events::{MidiEvent, MidiOutMessage};
use crate::vocabulary::limits::{CHANNEL_MAX, DATA_BYTE_MAX};

pub(crate) const NOTE_ON_STATUS: u8 = 0x90;
const NOTE_OFF_STATUS: u8 = 0x80;
const CONTROL_CHANGE_STATUS: u8 = 0xB0;

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
        NOTE_ON_STATUS => {
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
        0xC0 => {
            if data.len() < 2 {
                return Ok(None);
            }
            Ok(Some(MidiEvent::ProgramChange {
                program: data[1],
                channel,
            }))
        }
        0xE0 => {
            if data.len() < 3 {
                return Ok(None);
            }
            let value = (u16::from(data[2]) << 7) | u16::from(data[1]);
            Ok(Some(MidiEvent::PitchBend { value, channel }))
        }
        _ => Ok(None),
    }
}

fn in_range(value: u8, max: i64) -> Result<u8, MidiError> {
    if i64::from(value) <= max {
        Ok(value)
    } else {
        Err(MidiError::ValueOutOfRange { value, max })
    }
}

fn channel_message(status: u8, channel: u8, first: u8, second: u8) -> Result<Vec<u8>, MidiError> {
    Ok(vec![
        status | in_range(channel, CHANNEL_MAX)?,
        in_range(first, DATA_BYTE_MAX)?,
        in_range(second, DATA_BYTE_MAX)?,
    ])
}

pub(crate) fn message_to_bytes(message: &MidiOutMessage) -> Result<Vec<u8>, MidiError> {
    match message {
        MidiOutMessage::NoteOn {
            note,
            velocity,
            channel,
        } => channel_message(NOTE_ON_STATUS, *channel, *note, *velocity),
        MidiOutMessage::NoteOff {
            note,
            velocity,
            channel,
        } => channel_message(NOTE_OFF_STATUS, *channel, *note, *velocity),
        MidiOutMessage::ControlChange {
            controller,
            value,
            channel,
        } => channel_message(CONTROL_CHANGE_STATUS, *channel, *controller, *value),
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
        let result = decode_midi_bytes(&[0xD0, 64]).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn pitch_bend_full_down_decodes_to_zero() {
        let result = decode_midi_bytes(&[0xE0, 0x00, 0x00]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::PitchBend {
                value: 0,
                channel: 0,
            })
        );
    }

    #[test]
    fn pitch_bend_center_decodes_to_8192() {
        let result = decode_midi_bytes(&[0xE0, 0x00, 0x40]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::PitchBend {
                value: 8192,
                channel: 0,
            })
        );
    }

    #[test]
    fn pitch_bend_full_up_decodes_to_16383() {
        let result = decode_midi_bytes(&[0xE0, 0x7F, 0x7F]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::PitchBend {
                value: 16383,
                channel: 0,
            })
        );
    }

    #[test]
    fn pitch_bend_extracts_channel() {
        let result = decode_midi_bytes(&[0xE5, 0x00, 0x40]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::PitchBend {
                value: 8192,
                channel: 5,
            })
        );
    }

    #[test]
    fn pitch_bend_truncated_returns_none() {
        let result = decode_midi_bytes(&[0xE0, 0x00]).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn program_change_decodes_program() {
        let result = decode_midi_bytes(&[0xC0, 10]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::ProgramChange {
                program: 10,
                channel: 0,
            })
        );
    }

    #[test]
    fn program_change_extracts_channel() {
        let result = decode_midi_bytes(&[0xC3, 10]).unwrap();
        assert_eq!(
            result,
            Some(MidiEvent::ProgramChange {
                program: 10,
                channel: 3,
            })
        );
    }

    #[test]
    fn program_change_truncated_returns_none() {
        let result = decode_midi_bytes(&[0xC0]).unwrap();
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

    fn channel_messages(channel: u8, first: u8, second: u8) -> [MidiOutMessage; 3] {
        [
            MidiOutMessage::NoteOn {
                note: first,
                velocity: second,
                channel,
            },
            MidiOutMessage::NoteOff {
                note: first,
                velocity: second,
                channel,
            },
            MidiOutMessage::ControlChange {
                controller: first,
                value: second,
                channel,
            },
        ]
    }

    #[test]
    fn message_to_bytes_accepts_the_top_of_every_range_for_each_channel_message() {
        for (message, status) in channel_messages(15, 127, 127)
            .into_iter()
            .zip([0x9F, 0x8F, 0xBF])
        {
            assert_eq!(
                message_to_bytes(&message).unwrap(),
                vec![status, 127, 127],
                "{message:?}"
            );
        }
    }

    #[test]
    fn message_to_bytes_refuses_one_past_each_range_for_each_channel_message() {
        for (channel, first, second, refused, max) in [
            (16, 127, 127, 16, 15),
            (15, 128, 127, 128, 127),
            (15, 127, 128, 128, 127),
            (255, 255, 255, 255, 15),
        ] {
            for message in channel_messages(channel, first, second) {
                let error = message_to_bytes(&message).unwrap_err();
                assert!(
                    matches!(
                        error,
                        MidiError::ValueOutOfRange { value, max: limit }
                            if value == refused && limit == max
                    ),
                    "{message:?}: {error:?}"
                );
            }
        }
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
