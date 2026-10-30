use forge_types::{MessageRef, SubActionOutcome};

use crate::error::VTubeError;
use crate::vocabulary::messages;

pub(crate) fn message(error: &VTubeError) -> MessageRef {
    match error {
        VTubeError::Connect(detail) => messages::connect_failed_with(detail),
        VTubeError::ApiDisabled => messages::api_disabled(),
        VTubeError::TokenDenied => messages::access_denied(),
        VTubeError::TokenTimeout => messages::approval_timeout(),
        VTubeError::TokenRejected => messages::token_rejected(),
        VTubeError::Request { message } => messages::request_failed(message),
        VTubeError::Rejected { error_id, message } => {
            messages::request_rejected(*error_id, message)
        }
        VTubeError::NotConnected => messages::not_connected(),
        VTubeError::Timeout => messages::request_timed_out(),
        VTubeError::Json(source) => messages::payload_invalid(&source.to_string()),
    }
}

pub(crate) fn failed(error: &VTubeError) -> SubActionOutcome {
    SubActionOutcome::Failed(message(error).into())
}

pub(crate) fn outcome<T>(result: &Result<T, VTubeError>) -> SubActionOutcome {
    match result {
        Ok(_) => SubActionOutcome::Success,
        Err(error) => failed(error),
    }
}
#[cfg(test)]
mod tests {
    use forge_types::{MessageArg, UserText};

    use super::*;

    fn json_error() -> serde_json::Error {
        serde_json::from_str::<serde_json::Value>("{").unwrap_err()
    }

    #[test]
    fn every_vtube_error_variant_maps_to_its_own_code_with_the_upstream_text_as_detail() {
        let json = json_error();
        let json_text = json.to_string();
        let cases: Vec<(VTubeError, &str, Option<String>)> = vec![
            (
                VTubeError::Connect("refused by peer".to_owned()),
                "msg-vtube-connect_failed",
                Some("refused by peer".to_owned()),
            ),
            (VTubeError::ApiDisabled, "msg-vtube-api_disabled", None),
            (VTubeError::TokenDenied, "msg-vtube-access_denied", None),
            (VTubeError::TokenTimeout, "msg-vtube-approval_timeout", None),
            (VTubeError::TokenRejected, "msg-vtube-token_rejected", None),
            (
                VTubeError::Request {
                    message: "socket reset".to_owned(),
                },
                "msg-vtube-request_failed",
                Some("socket reset".to_owned()),
            ),
            (
                VTubeError::Rejected {
                    error_id: 50,
                    message: "Hotkey not found".to_owned(),
                },
                "msg-vtube-request_rejected",
                Some("Hotkey not found".to_owned()),
            ),
            (VTubeError::NotConnected, "msg-vtube-not_connected", None),
            (VTubeError::Timeout, "msg-vtube-request_timed_out", None),
            (
                VTubeError::Json(json),
                "msg-vtube-payload_invalid",
                Some(json_text),
            ),
        ];

        for (error, expected_key, expected_detail) in cases {
            let mapped = message(&error);

            assert_eq!(mapped.code().ftl_key(), expected_key, "{error}");
            assert_eq!(
                mapped.detail().map(|d| d.as_str().to_owned()),
                expected_detail,
                "{error}"
            );
        }
    }

    #[test]
    fn a_rejected_request_carries_its_error_id_as_an_argument_and_not_in_the_detail() {
        let mapped = message(&VTubeError::Rejected {
            error_id: 50,
            message: "Hotkey not found".to_owned(),
        });

        assert_eq!(mapped.arg("error_id"), Some(&MessageArg::Int(50)));
        assert_eq!(mapped.args().count(), 1);
        assert!(!mapped.detail().unwrap().as_str().contains("50"));
    }

    #[test]
    fn variants_without_upstream_text_carry_no_detail_and_no_arguments() {
        for error in [
            VTubeError::ApiDisabled,
            VTubeError::TokenDenied,
            VTubeError::TokenTimeout,
            VTubeError::TokenRejected,
            VTubeError::NotConnected,
            VTubeError::Timeout,
        ] {
            let mapped = message(&error);

            assert!(mapped.detail().is_none(), "{error}");
            assert_eq!(mapped.args().count(), 0, "{error}");
        }
    }

    #[test]
    fn a_failed_result_becomes_a_catalogued_failure_and_an_ok_result_succeeds() {
        let failed_outcome = outcome::<()>(&Err(VTubeError::Timeout));
        let ok_outcome = outcome(&Ok(()));

        assert!(matches!(
            failed_outcome,
            SubActionOutcome::Failed(UserText::Message(ref m))
                if m.code().ftl_key() == "msg-vtube-request_timed_out"
        ));
        assert_eq!(ok_outcome, SubActionOutcome::Success);
    }
}
