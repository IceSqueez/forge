use forge_types::MessageCode;

use crate::vocabulary::INTEGRATION_ID;

pub(crate) const CLIENT_NAME: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_client_name");
pub(crate) const DISABLED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_disabled");
pub(crate) const EVENTS_RECEIVED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_events_received");
pub(crate) const INPUT_PORTS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_input_ports");
pub(crate) const OPEN_FAILED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_open_failed");
pub(crate) const OUTPUT_PORTS: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_output_ports");
pub(crate) const POLL_INTERVAL: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_poll_interval");
pub(crate) const POLL_INTERVAL_VALUE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_poll_interval_value");
pub(crate) const SEND_CC: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_send_cc");
pub(crate) const SEND_NOTE_OFF: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_send_note_off");
pub(crate) const SEND_NOTE_ON: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_send_note_on");
pub(crate) const SEND_RAW: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_send_raw");
pub(crate) const STATUS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "label_status");
pub(crate) const SUBSCRIBED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "label_subscribed");
