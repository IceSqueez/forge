use forge_types::{MessageRef, SubActionOutcome};

use crate::error::MidiError;
use crate::vocabulary::messages;

pub(crate) fn message(error: &MidiError) -> MessageRef {
    match error {
        MidiError::PortNotFound { name } => messages::port_not_found(name),
        MidiError::PortDisconnected { name } => messages::port_disconnected(name),
        MidiError::InvalidStatusByte(byte) => messages::invalid_status_byte(*byte),
        MidiError::ValueOutOfRange { value, max } => messages::value_out_of_range(*value, *max),
        MidiError::MidirInit(detail) => messages::backend_unavailable_with(detail),
        MidiError::MidirConnect(detail) => messages::port_connect_failed(detail),
        MidiError::OutputSend(detail) => messages::send_failed(detail),
        MidiError::SupervisorUnavailable => messages::supervisor_unavailable(),
    }
}

pub(crate) fn outcome(result: &Result<(), MidiError>) -> SubActionOutcome {
    match result {
        Ok(()) => SubActionOutcome::Success,
        Err(error) => SubActionOutcome::Failed(message(error).into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocabulary::messages::MIDI_MESSAGES;
    use forge_types::{MessageArg, UserText, VerbatimText};

    fn every_error_with_its_code() -> Vec<(MidiError, &'static str)> {
        vec![
            (
                MidiError::PortNotFound { name: "Pad".into() },
                "midi.port_not_found",
            ),
            (
                MidiError::PortDisconnected { name: "Pad".into() },
                "midi.port_disconnected",
            ),
            (
                MidiError::InvalidStatusByte(0x7F),
                "midi.invalid_status_byte",
            ),
            (
                MidiError::ValueOutOfRange {
                    value: 200,
                    max: 127,
                },
                "midi.value_out_of_range",
            ),
            (
                MidiError::MidirInit("alsa down".into()),
                "midi.backend_unavailable",
            ),
            (
                MidiError::MidirConnect("busy".into()),
                "midi.port_connect_failed",
            ),
            (MidiError::OutputSend("pipe".into()), "midi.send_failed"),
            (
                MidiError::SupervisorUnavailable,
                "midi.supervisor_unavailable",
            ),
        ]
    }

    #[test]
    fn every_midi_error_maps_to_its_own_catalogued_code() {
        for (error, expected) in every_error_with_its_code() {
            let code = message(&error).code().to_string();

            assert_eq!(code, expected, "{error:?}");
            assert!(
                MIDI_MESSAGES.codes.iter().any(|c| c.to_string() == code),
                "{code} not catalogued"
            );
        }
    }

    #[test]
    fn port_errors_carry_the_port_name_as_a_verbatim_argument() {
        for error in [
            MidiError::PortNotFound { name: "Pad".into() },
            MidiError::PortDisconnected { name: "Pad".into() },
        ] {
            assert_eq!(
                message(&error).arg("port"),
                Some(&MessageArg::Verbatim(VerbatimText::new("Pad")))
            );
        }
    }

    #[test]
    fn an_invalid_status_byte_is_carried_as_its_numeric_value() {
        assert_eq!(
            message(&MidiError::InvalidStatusByte(0x7F)).arg("byte"),
            Some(&MessageArg::Int(0x7F))
        );
    }

    #[test]
    fn an_out_of_range_value_carries_the_value_and_its_maximum() {
        let mapped = message(&MidiError::ValueOutOfRange {
            value: 200,
            max: 127,
        });

        assert_eq!(
            (mapped.arg("value"), mapped.arg("max")),
            (Some(&MessageArg::Int(200)), Some(&MessageArg::Int(127)))
        );
    }

    #[test]
    fn backend_errors_keep_the_upstream_text_as_detail_not_as_an_argument() {
        for error in [
            MidiError::MidirInit("alsa down".into()),
            MidiError::MidirConnect("alsa down".into()),
            MidiError::OutputSend("alsa down".into()),
        ] {
            let mapped = message(&error);

            assert_eq!(mapped.detail().map(|d| d.as_str()), Some("alsa down"));
            assert_eq!(mapped.args().count(), 0);
        }
    }

    #[test]
    fn an_error_without_upstream_text_carries_no_detail() {
        assert!(
            message(&MidiError::SupervisorUnavailable)
                .detail()
                .is_none()
        );
    }

    #[test]
    fn a_successful_send_is_a_success_and_a_failed_one_carries_the_mapped_message() {
        assert!(matches!(outcome(&Ok(())), SubActionOutcome::Success));
        assert_eq!(
            outcome(&Err(MidiError::SupervisorUnavailable)),
            SubActionOutcome::Failed(UserText::Message(message(
                &MidiError::SupervisorUnavailable
            )))
        );
    }
}
