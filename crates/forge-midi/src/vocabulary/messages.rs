use forge_types::{
    MessageArg, MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText,
};
use time::OffsetDateTime;

use crate::vocabulary::INTEGRATION_ID;
use crate::vocabulary::labels;

pub const BACKEND_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "backend_unavailable");

pub const RAW_BYTES_SHAPE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "raw_bytes_shape");

const PORT_NOT_FOUND: MessageCode = MessageCode::from_static(INTEGRATION_ID, "port_not_found");
const PORT_DISCONNECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "port_disconnected");
const INVALID_STATUS_BYTE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "invalid_status_byte");
const PORT_CONNECT_FAILED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "port_connect_failed");
const SEND_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "send_failed");
const SUPERVISOR_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "supervisor_unavailable");

const PORT_ARG: &str = "port";
const BYTE_ARG: &str = "byte";
const TIME_ARG: &str = "time";

const HEALTH_INPUT_CONNECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_input_connected");
const HEALTH_DISABLED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_disabled");
const HEALTH_OUTPUT_AVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_output_available");
const HEALTH_NOTE_TIME: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_note_time");
const HEALTH_LAST_MINUTE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_minute");

pub const HEALTH_INPUT_PORTS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_input_ports");
pub const HEALTH_OUTPUT_PORTS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_output_ports");
pub const HEALTH_LAST_NOTE_ON: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_note_on");
pub const HEALTH_EVENTS_PER_MINUTE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_events_per_minute");

pub(crate) const CONDITION_ANY_NOTE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_note");
pub(crate) const CONDITION_ANY_CONTROLLER: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_controller");
pub(crate) const CONDITION_ANY_PROGRAM: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_program");
pub(crate) const CONDITION_ANY_CHANNEL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_channel");
pub(crate) const CONDITION_ANY_DIRECTION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_any_direction");
const CONDITION_DEVICE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "condition_device");
const CONDITION_DIRECTION: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_direction");
pub(crate) const CONDITION_NOTE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_note");
pub(crate) const CONDITION_CONTROLLER: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_controller");
pub(crate) const CONDITION_PROGRAM: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_program");
const CONDITION_CHANNEL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "condition_channel");
const DEVICE_CONDITION_ARG: &str = "device";
const DIRECTION_CONDITION_ARG: &str = "direction";
pub(crate) const NUMBER_CONDITION_ARG: &str = "number";
const CHANNEL_CONDITION_ARG: &str = "channel";

pub const MIDI_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_INPUT_PORTS,
        HEALTH_OUTPUT_PORTS,
        HEALTH_LAST_NOTE_ON,
        HEALTH_EVENTS_PER_MINUTE,
        HEALTH_INPUT_CONNECTED,
        HEALTH_DISABLED,
        HEALTH_OUTPUT_AVAILABLE,
        HEALTH_NOTE_TIME,
        HEALTH_LAST_MINUTE,
        BACKEND_UNAVAILABLE,
        RAW_BYTES_SHAPE,
        PORT_NOT_FOUND,
        PORT_DISCONNECTED,
        INVALID_STATUS_BYTE,
        PORT_CONNECT_FAILED,
        SEND_FAILED,
        SUPERVISOR_UNAVAILABLE,
        CONDITION_ANY_NOTE,
        CONDITION_ANY_CONTROLLER,
        CONDITION_ANY_PROGRAM,
        CONDITION_ANY_CHANNEL,
        CONDITION_ANY_DIRECTION,
        CONDITION_DEVICE,
        CONDITION_DIRECTION,
        CONDITION_NOTE,
        CONDITION_CONTROLLER,
        CONDITION_PROGRAM,
        CONDITION_CHANNEL,
        labels::CLIENT_NAME,
        labels::DISABLED,
        labels::EVENTS_RECEIVED,
        labels::INPUT_PORTS,
        labels::OPEN_FAILED,
        labels::OUTPUT_PORTS,
        labels::POLL_INTERVAL,
        labels::POLL_INTERVAL_VALUE,
        labels::SEND_CC,
        labels::SEND_NOTE_OFF,
        labels::SEND_NOTE_ON,
        labels::SEND_RAW,
        labels::STATUS,
        labels::SUBSCRIBED,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

pub(crate) fn health_input_connected() -> MessageRef {
    MessageRef::new(HEALTH_INPUT_CONNECTED)
}

pub(crate) fn health_disabled() -> MessageRef {
    MessageRef::new(HEALTH_DISABLED)
}

pub(crate) fn health_output_available() -> MessageRef {
    MessageRef::new(HEALTH_OUTPUT_AVAILABLE)
}

pub(crate) fn health_note_time(at: OffsetDateTime) -> MessageRef {
    MessageRef::new(HEALTH_NOTE_TIME).with_arg(TIME_ARG, MessageArg::TimeOfDay(at))
}

pub(crate) fn health_last_minute() -> MessageRef {
    MessageRef::new(HEALTH_LAST_MINUTE)
}

pub(crate) fn backend_unavailable_with(detail: &str) -> MessageRef {
    MessageRef::new(BACKEND_UNAVAILABLE).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn port_not_found(port: &str) -> MessageRef {
    MessageRef::new(PORT_NOT_FOUND).with_arg(PORT_ARG, VerbatimText::new(port))
}

pub(crate) fn port_disconnected(port: &str) -> MessageRef {
    MessageRef::new(PORT_DISCONNECTED).with_arg(PORT_ARG, VerbatimText::new(port))
}

pub(crate) fn invalid_status_byte(byte: u8) -> MessageRef {
    MessageRef::new(INVALID_STATUS_BYTE).with_arg(BYTE_ARG, i64::from(byte))
}

pub(crate) fn port_connect_failed(detail: &str) -> MessageRef {
    MessageRef::new(PORT_CONNECT_FAILED).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn send_failed(detail: &str) -> MessageRef {
    MessageRef::new(SEND_FAILED).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn supervisor_unavailable() -> MessageRef {
    MessageRef::new(SUPERVISOR_UNAVAILABLE)
}

pub(crate) fn condition_device(device: &str) -> MessageRef {
    MessageRef::new(CONDITION_DEVICE).with_arg(DEVICE_CONDITION_ARG, VerbatimText::new(device))
}

pub(crate) fn condition_direction(direction: &str) -> MessageRef {
    MessageRef::new(CONDITION_DIRECTION)
        .with_arg(DIRECTION_CONDITION_ARG, VerbatimText::new(direction))
}

pub(crate) fn condition_channel(stored_channel: i64) -> MessageRef {
    MessageRef::new(CONDITION_CHANNEL).with_arg(CHANNEL_CONDITION_ARG, stored_channel + 1)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    fn ftl_variables(source: &str) -> BTreeMap<String, BTreeSet<String>> {
        let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut current = None;
        for line in source.lines() {
            if let Some((id, _)) = line.split_once(" = ").filter(|_| line.starts_with("msg-")) {
                current = Some(id.trim().to_owned());
                found.entry(id.trim().to_owned()).or_default();
            }
            let Some(id) = &current else { continue };
            let mut rest = line;
            while let Some(at) = rest.find('$') {
                rest = &rest[at + 1..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                found.get_mut(id).unwrap().insert(name);
            }
        }
        found
    }

    fn every_constructor() -> Vec<MessageRef> {
        vec![
            MessageRef::new(HEALTH_INPUT_PORTS),
            MessageRef::new(HEALTH_OUTPUT_PORTS),
            MessageRef::new(HEALTH_LAST_NOTE_ON),
            MessageRef::new(HEALTH_EVENTS_PER_MINUTE),
            health_input_connected(),
            health_disabled(),
            health_output_available(),
            health_note_time(OffsetDateTime::UNIX_EPOCH),
            health_last_minute(),
            MessageRef::new(BACKEND_UNAVAILABLE),
            MessageRef::new(RAW_BYTES_SHAPE),
            backend_unavailable_with("c"),
            port_not_found("p"),
            port_disconnected("p"),
            invalid_status_byte(1),
            port_connect_failed("c"),
            send_failed("c"),
            supervisor_unavailable(),
            MessageRef::new(CONDITION_ANY_NOTE),
            MessageRef::new(CONDITION_ANY_CONTROLLER),
            MessageRef::new(CONDITION_ANY_PROGRAM),
            MessageRef::new(CONDITION_ANY_CHANNEL),
            MessageRef::new(CONDITION_ANY_DIRECTION),
            MessageRef::new(CONDITION_NOTE).with_arg(NUMBER_CONDITION_ARG, 1),
            MessageRef::new(CONDITION_CONTROLLER).with_arg(NUMBER_CONDITION_ARG, 1),
            MessageRef::new(CONDITION_PROGRAM).with_arg(NUMBER_CONDITION_ARG, 1),
            condition_device("d"),
            condition_direction("input"),
            condition_channel(1),
        ]
    }

    #[test]
    fn every_constructor_code_is_catalogued_and_localized_in_both_languages_with_matching_variables()
     {
        let en = ftl_variables(MIDI_MESSAGES.en_ftl);
        let uk = ftl_variables(MIDI_MESSAGES.uk_ftl);
        for message in every_constructor() {
            let code = message.code();
            assert!(MIDI_MESSAGES.codes.contains(code), "{code} not catalogued");
            let passed: BTreeSet<String> = message.args().map(|(n, _)| n.to_owned()).collect();
            for (language, ftl) in [("en", &en), ("uk", &uk)] {
                let used = ftl
                    .get(&code.ftl_key())
                    .unwrap_or_else(|| panic!("{code} missing in {language}"));
                assert_eq!(used, &passed, "{code} variables in {language}");
            }
        }
    }

    #[test]
    fn every_catalogued_code_has_a_constructor_in_the_table() {
        let produced: BTreeSet<MessageCode> = every_constructor()
            .iter()
            .map(|m| m.code().clone())
            .collect();
        let catalogued: BTreeSet<MessageCode> = MIDI_MESSAGES
            .codes
            .iter()
            .filter(|code| !code.name().starts_with("label_"))
            .cloned()
            .collect();
        assert_eq!(produced, catalogued);
    }

    #[test]
    fn english_and_ukrainian_declare_the_same_messages_with_the_same_variables() {
        assert_eq!(
            ftl_variables(MIDI_MESSAGES.en_ftl),
            ftl_variables(MIDI_MESSAGES.uk_ftl)
        );
    }

    #[test]
    fn upstream_cause_text_is_carried_as_detail_and_never_as_an_argument() {
        for failure in [
            send_failed("busy: secret-path"),
            port_connect_failed("busy: secret-path"),
            backend_unavailable_with("busy: secret-path"),
        ] {
            assert_eq!(
                failure.detail().map(|d| d.as_str()),
                Some("busy: secret-path")
            );
            assert!(
                failure
                    .args()
                    .all(|(_, v)| !format!("{v:?}").contains("secret-path"))
            );
        }
    }
}
