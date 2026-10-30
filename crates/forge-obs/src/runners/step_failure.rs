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

#[cfg(test)]
mod tests {
    use forge_types::UserText;

    use super::*;

    fn payload_error() -> ObsError {
        ObsError::Payload(serde_json::from_str::<serde_json::Value>("{").unwrap_err())
    }

    #[test]
    fn every_obs_error_variant_maps_to_its_own_code_with_the_upstream_text_as_detail() {
        let payload = payload_error();
        let payload_source = match &payload {
            ObsError::Payload(source) => source.to_string(),
            _ => unreachable!(),
        };
        let cases: Vec<(ObsError, &str, Option<String>)> = vec![
            (
                ObsError::Connect("refused by peer".to_owned()),
                "msg-obs-connect_failed",
                Some("refused by peer".to_owned()),
            ),
            (ObsError::Authentication, "msg-obs-password_rejected", None),
            (ObsError::Disconnected, "msg-obs-not_connected", None),
            (ObsError::Timeout, "msg-obs-request_timed_out", None),
            (
                ObsError::Protocol("bad op code".to_owned()),
                "msg-obs-protocol_error",
                Some("bad op code".to_owned()),
            ),
            (payload, "msg-obs-payload_invalid", Some(payload_source)),
            (
                ObsError::StudioModeInactive,
                "msg-obs-studio_mode_inactive",
                None,
            ),
            (
                ObsError::Request {
                    request_type: "SetCurrentProgramScene".to_owned(),
                    message: "ResourceNotFound".to_owned(),
                },
                "msg-obs-request_rejected",
                Some("ResourceNotFound".to_owned()),
            ),
        ];

        for (error, expected_key, expected_detail) in cases {
            let message = message(&error);

            assert_eq!(message.code().ftl_key(), expected_key, "{error}");
            assert_eq!(
                message.detail().map(|d| d.as_str().to_owned()),
                expected_detail,
                "{error}"
            );
        }
    }

    #[test]
    fn a_rejected_request_names_the_request_type_as_an_argument_not_in_the_detail() {
        let error = ObsError::Request {
            request_type: "SetCurrentProgramScene".to_owned(),
            message: "ResourceNotFound".to_owned(),
        };

        let message = message(&error);

        assert_eq!(
            message.args().map(|(name, _)| name).collect::<Vec<_>>(),
            vec!["request"]
        );
        assert!(
            !message
                .detail()
                .unwrap()
                .as_str()
                .contains("SetCurrentProgramScene")
        );
    }

    #[test]
    fn a_rejected_password_carries_no_detail_and_no_arguments_so_nothing_secret_can_ride_along() {
        let message = message(&ObsError::Authentication);

        assert!(message.detail().is_none());
        assert_eq!(message.args().count(), 0);
    }

    #[test]
    fn a_failed_result_becomes_a_catalogued_failure_and_an_ok_result_succeeds() {
        let failed_outcome = outcome::<()>(&Err(ObsError::Timeout));
        let ok_outcome = outcome(&Ok(()));

        assert!(matches!(
            failed_outcome,
            SubActionOutcome::Failed(UserText::Message(ref m))
                if m.code().ftl_key() == "msg-obs-request_timed_out"
        ));
        assert_eq!(ok_outcome, SubActionOutcome::Success);
    }
}
