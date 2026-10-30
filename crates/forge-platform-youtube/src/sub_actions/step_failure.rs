use forge_platform_core::PlatformError;
use forge_types::{MessageRef, SubActionOutcome, UserText};

use crate::vocabulary::messages;

pub(super) fn platform_message(error: &PlatformError) -> MessageRef {
    match error {
        PlatformError::RateLimited { retry_after_secs } => {
            messages::rate_limited(*retry_after_secs)
        }
        PlatformError::RateLimitExhausted => messages::rate_budget_exhausted(),
        PlatformError::QuotaExhausted => messages::quota_exhausted(),
        PlatformError::ReauthRequired { .. } => messages::reauth_required(),
        PlatformError::Auth { reason } => messages::auth_refused(reason),
        PlatformError::Http { status, body } => messages::request_refused(*status, body),
        PlatformError::MalformedResponse { reason } => messages::response_unreadable(reason),
        PlatformError::Serialization(error) => messages::response_unreadable(&error.to_string()),
        PlatformError::Network { reason } => messages::request_failed(reason),
        PlatformError::Io(_)
        | PlatformError::LoopbackPortInUse { .. }
        | PlatformError::Unsupported { .. }
        | PlatformError::EndpointOverrideRefused { .. } => {
            messages::request_failed(&error.to_string())
        }
    }
}

pub(super) fn failed(error: &PlatformError) -> UserText {
    platform_message(error).into()
}

pub(super) fn outcome<T>(result: &Result<T, PlatformError>) -> SubActionOutcome {
    match result {
        Ok(_) => SubActionOutcome::Success,
        Err(error) => SubActionOutcome::Failed(failed(error)),
    }
}
#[cfg(test)]
mod tests {
    use forge_platform_core::EndpointRefusal;
    use forge_types::MessageArg;

    use super::*;

    const UPSTREAM_TEXT: &str = "upstream-sentinel-text";

    fn code(message: &MessageRef) -> String {
        message.code().to_string()
    }

    fn detail(message: &MessageRef) -> Option<&str> {
        message.detail().map(|d| d.as_str())
    }

    #[test]
    fn each_platform_error_maps_to_its_own_code_with_arguments_and_detail_only_where_they_exist() {
        let serialization = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let cases: Vec<(PlatformError, &str, Option<MessageArg>, bool)> = vec![
            (
                PlatformError::RateLimited {
                    retry_after_secs: 12,
                },
                "youtube.rate_limited",
                Some(MessageArg::Int(12)),
                false,
            ),
            (
                PlatformError::RateLimitExhausted,
                "youtube.rate_budget_exhausted",
                None,
                false,
            ),
            (
                PlatformError::QuotaExhausted,
                "youtube.quota_exhausted",
                None,
                false,
            ),
            (
                PlatformError::ReauthRequired {
                    platform: UPSTREAM_TEXT.into(),
                },
                "youtube.reauth_required",
                None,
                false,
            ),
            (
                PlatformError::Auth {
                    reason: UPSTREAM_TEXT.into(),
                },
                "youtube.auth_refused",
                None,
                true,
            ),
            (
                PlatformError::Http {
                    status: 503,
                    body: UPSTREAM_TEXT.into(),
                },
                "youtube.request_refused",
                Some(MessageArg::Int(503)),
                true,
            ),
            (
                PlatformError::MalformedResponse {
                    reason: UPSTREAM_TEXT.into(),
                },
                "youtube.response_unreadable",
                None,
                true,
            ),
            (
                PlatformError::Serialization(serialization),
                "youtube.response_unreadable",
                None,
                true,
            ),
            (
                PlatformError::Network {
                    reason: UPSTREAM_TEXT.into(),
                },
                "youtube.request_failed",
                None,
                true,
            ),
            (
                PlatformError::Io(std::io::Error::other(UPSTREAM_TEXT)),
                "youtube.request_failed",
                None,
                true,
            ),
            (
                PlatformError::LoopbackPortInUse { port: 8080 },
                "youtube.request_failed",
                None,
                true,
            ),
            (
                PlatformError::Unsupported {
                    feature: "x".into(),
                },
                "youtube.request_failed",
                None,
                true,
            ),
            (
                PlatformError::EndpointOverrideRefused {
                    variable: "V",
                    reason: EndpointRefusal::Malformed,
                },
                "youtube.request_failed",
                None,
                true,
            ),
        ];
        for (error, expected_code, first_arg, has_detail) in cases {
            let message = platform_message(&error);

            assert_eq!(code(&message), expected_code, "{error:?}");
            assert_eq!(detail(&message).is_some(), has_detail, "{error:?}");
            assert_eq!(
                message.args().next().map(|(_, v)| v.clone()),
                first_arg,
                "{error:?}"
            );
        }
    }

    #[test]
    fn upstream_text_is_only_ever_the_detail_never_an_argument() {
        let message = platform_message(&PlatformError::Network {
            reason: UPSTREAM_TEXT.into(),
        });

        assert_eq!(detail(&message), Some(UPSTREAM_TEXT));
        assert!(
            message
                .args()
                .all(|(_, v)| !format!("{v:?}").contains(UPSTREAM_TEXT))
        );
    }

    #[test]
    fn outcome_is_success_only_for_ok_and_failed_carries_the_mapped_message() {
        assert_eq!(
            outcome::<()>(&Err(PlatformError::QuotaExhausted)),
            SubActionOutcome::Failed(failed(&PlatformError::QuotaExhausted))
        );
        assert_eq!(outcome(&Ok(())), SubActionOutcome::Success);
    }
}
