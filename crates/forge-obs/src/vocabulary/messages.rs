use forge_types::{MessageCatalog, MessageCode, MessageRef, UpstreamDetail, VerbatimText};

use crate::vocabulary::INTEGRATION_ID;

const CONNECT_REFUSED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_refused");
const UNREACHABLE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unreachable");
const PASSWORD_REJECTED: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "password_rejected");
const CONNECT_FAILED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connect_failed");
const CONNECTION_LOST: MessageCode = MessageCode::from_static(INTEGRATION_ID, "connection_lost");
const UNRESPONSIVE: MessageCode = MessageCode::from_static(INTEGRATION_ID, "unresponsive");
const NOT_CONNECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "not_connected");
const REQUEST_TIMED_OUT: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "request_timed_out");
const REQUEST_REJECTED: MessageCode = MessageCode::from_static(INTEGRATION_ID, "request_rejected");
const PROTOCOL_ERROR: MessageCode = MessageCode::from_static(INTEGRATION_ID, "protocol_error");
const PAYLOAD_INVALID: MessageCode = MessageCode::from_static(INTEGRATION_ID, "payload_invalid");
const STUDIO_MODE_INACTIVE: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "studio_mode_inactive");

const REQUEST_ARG: &str = "request";

pub const HEALTH_STREAM: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_stream");
pub const HEALTH_RECORDING: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_recording");
pub const HEALTH_CPU_FPS: MessageCode = MessageCode::from_static(INTEGRATION_ID, "health_cpu_fps");
pub const HEALTH_DROPPED_FRAMES: MessageCode =
    MessageCode::from_static(INTEGRATION_ID, "health_dropped_frames");

pub const OBS_MESSAGES: MessageCatalog = MessageCatalog {
    namespace: INTEGRATION_ID,
    codes: &[
        HEALTH_STREAM,
        HEALTH_RECORDING,
        HEALTH_CPU_FPS,
        HEALTH_DROPPED_FRAMES,
        CONNECT_REFUSED,
        UNREACHABLE,
        PASSWORD_REJECTED,
        CONNECT_FAILED,
        CONNECTION_LOST,
        UNRESPONSIVE,
        NOT_CONNECTED,
        REQUEST_TIMED_OUT,
        REQUEST_REJECTED,
        PROTOCOL_ERROR,
        PAYLOAD_INVALID,
        STUDIO_MODE_INACTIVE,
    ],
    en_ftl: include_str!("../../locales/en/messages.ftl"),
    uk_ftl: include_str!("../../locales/uk/messages.ftl"),
};

pub(crate) fn connect_refused() -> MessageRef {
    MessageRef::new(CONNECT_REFUSED)
}

pub(crate) fn unreachable() -> MessageRef {
    MessageRef::new(UNREACHABLE)
}

pub(crate) fn password_rejected() -> MessageRef {
    MessageRef::new(PASSWORD_REJECTED)
}

pub(crate) fn connect_failed() -> MessageRef {
    MessageRef::new(CONNECT_FAILED)
}

pub(crate) fn connect_failed_with(detail: &str) -> MessageRef {
    connect_failed().with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn not_connected() -> MessageRef {
    MessageRef::new(NOT_CONNECTED)
}

pub(crate) fn request_timed_out() -> MessageRef {
    MessageRef::new(REQUEST_TIMED_OUT)
}

pub(crate) fn request_rejected(request: &str, detail: &str) -> MessageRef {
    MessageRef::new(REQUEST_REJECTED)
        .with_arg(REQUEST_ARG, VerbatimText::new(request))
        .with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn protocol_error(detail: &str) -> MessageRef {
    MessageRef::new(PROTOCOL_ERROR).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn payload_invalid(detail: &str) -> MessageRef {
    MessageRef::new(PAYLOAD_INVALID).with_detail(UpstreamDetail::from_redacted(detail))
}

pub(crate) fn studio_mode_inactive() -> MessageRef {
    MessageRef::new(STUDIO_MODE_INACTIVE)
}

pub(crate) fn connection_lost() -> MessageRef {
    MessageRef::new(CONNECTION_LOST)
}

pub(crate) fn unresponsive() -> MessageRef {
    MessageRef::new(UNRESPONSIVE)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_the_crate_emits_is_listed_in_the_catalog() {
        for emitted in [
            connect_refused(),
            unreachable(),
            password_rejected(),
            connect_failed(),
            connection_lost(),
            unresponsive(),
        ] {
            assert!(
                OBS_MESSAGES.codes.contains(emitted.code()),
                "{} is emitted but not catalogued",
                emitted.code()
            );
        }
    }
}
