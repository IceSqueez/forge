use forge_types::{MessageRef, SubActionOutcome};

use crate::ObsError;
use crate::vocabulary::messages;

pub(crate) fn message(error: &ObsError) -> MessageRef {
    match error {
        ObsError::Connect(detail) => messages::connect_failed_with(detail),
        ObsError::Authentication => messages::password_rejected(),
        ObsError::Disconnected => messages::not_connected(),
        ObsError::Timeout => messages::request_timed_out(),
        ObsError::Protocol(detail) => messages::protocol_error(detail),
        ObsError::Payload(source) => messages::payload_invalid(&source.to_string()),
        ObsError::StudioModeInactive => messages::studio_mode_inactive(),
        ObsError::Request {
            request_type,
            message,
        } => messages::request_rejected(request_type, message),
    }
}

pub(crate) fn failed(error: &ObsError) -> SubActionOutcome {
    SubActionOutcome::Failed(message(error).into())
}

pub(crate) fn outcome<T>(result: &Result<T, ObsError>) -> SubActionOutcome {
    match result {
        Ok(_) => SubActionOutcome::Success,
        Err(error) => failed(error),
    }
}
