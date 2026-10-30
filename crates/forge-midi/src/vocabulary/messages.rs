use forge_types::{MessageCatalog, MessageCode};

use crate::vocabulary::INTEGRATION_ID;

pub const BACKEND_UNAVAILABLE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "backend_unavailable");

pub const HEALTH_INPUT_PORTS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_input_ports");
pub const HEALTH_OUTPUT_PORTS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_output_ports");
pub const HEALTH_LAST_NOTE_ON: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_last_note_on");
pub const HEALTH_EVENTS_PER_MINUTE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_events_per_minute");

pub const MIDI_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_INPUT_PORTS,
        HEALTH_OUTPUT_PORTS,
        HEALTH_LAST_NOTE_ON,
        HEALTH_EVENTS_PER_MINUTE,
        BACKEND_UNAVAILABLE,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};
