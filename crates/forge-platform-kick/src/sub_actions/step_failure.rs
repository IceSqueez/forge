use forge_platform_core::PlatformError;
use forge_types::{MessageRef, SubActionOutcome, UserText};

use crate::vocabulary::messages;

pub(crate) fn platform_message(error: &PlatformError) -> MessageRef {
    match error {
        PlatformError::RateLimited { retry_after_secs } => {
            messages::rate_limited(*retry_after_secs)
        }
        PlatformError::RateLimitExhausted | PlatformError::QuotaExhausted => {
            messages::rate_budget_exhausted()
        }
        PlatformError::ReauthRequired { .. } => messages::reauth_required(),
        PlatformError::Auth { reason } => messages::auth_refused(reason),
        PlatformError::DeviceAuthEnded(_) => messages::auth_refused(&error.to_string()),
        PlatformError::Http { status, .. } => messages::request_refused(*status),
        PlatformError::Refused { reason } => messages::request_failed(reason),
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

pub(crate) fn failed(error: &PlatformError) -> UserText {
    platform_message(error).into()
}

pub(crate) fn outcome<T>(result: &Result<T, PlatformError>) -> SubActionOutcome {
    match result {
        Ok(_) => SubActionOutcome::Success,
        Err(error) => SubActionOutcome::Failed(failed(error)),
    }
}

pub(crate) fn token_failed(error: &PlatformError) -> UserText {
    match error {
        PlatformError::ReauthRequired { .. } => messages::reauth_required().into(),
        other => messages::token_unavailable(&other.to_string()).into(),
    }
}

pub(crate) fn broadcaster_failed(error: &PlatformError) -> UserText {
    match error {
        PlatformError::ReauthRequired { .. } => messages::reauth_required().into(),
        other => messages::broadcaster_unavailable(&other.to_string()).into(),
    }
}

#[cfg(test)]
mod tests {
    use forge_types::MessageArg;

    use super::*;

    const UPSTREAM: &str = "upstream-secret-text";

    fn message(text: &UserText) -> &MessageRef {
        match text {
            UserText::Message(message) => message,
            UserText::Verbatim(text) => panic!("expected a coded message, got {text:?}"),
        }
    }

    fn io_error() -> PlatformError {
        PlatformError::Io(std::io::Error::other(UPSTREAM))
    }

    fn serialization_error() -> PlatformError {
        PlatformError::Serialization(serde_json::from_str::<u8>(UPSTREAM).unwrap_err())
    }

    #[test]
    fn every_platform_error_variant_maps_to_its_code() {
        let cases: Vec<(&str, PlatformError, &str)> = vec![
            (
                "rate limited",
                PlatformError::RateLimited {
                    retry_after_secs: 7,
                },
                "kick.rate_limited",
            ),
            (
                "client budget",
                PlatformError::RateLimitExhausted,
                "kick.rate_budget_exhausted",
            ),
            (
                "daily quota",
                PlatformError::QuotaExhausted,
                "kick.rate_budget_exhausted",
            ),
            (
                "reauth",
                PlatformError::ReauthRequired {
                    platform: "kick".into(),
                },
                "kick.reauth_required",
            ),
            (
                "auth",
                PlatformError::Auth {
                    reason: UPSTREAM.into(),
                },
                "kick.auth_refused",
            ),
            (
                "device sign-in ended",
                PlatformError::DeviceAuthEnded(forge_platform_core::DeviceAuthEnd::Expired),
                "kick.auth_refused",
            ),
            (
                "http",
                PlatformError::Http {
                    status: 403,
                    body: UPSTREAM.into(),
                },
                "kick.request_refused",
            ),
            (
                "malformed",
                PlatformError::MalformedResponse {
                    reason: UPSTREAM.into(),
                },
                "kick.response_unreadable",
            ),
            (
                "serialization",
                serialization_error(),
                "kick.response_unreadable",
            ),
            (
                "network",
                PlatformError::Network {
                    reason: UPSTREAM.into(),
                },
                "kick.request_failed",
            ),
            (
                "refused",
                PlatformError::Refused {
                    reason: UPSTREAM.into(),
                },
                "kick.request_failed",
            ),
            ("io", io_error(), "kick.request_failed"),
            (
                "loopback",
                PlatformError::LoopbackPortInUse { port: 1 },
                "kick.request_failed",
            ),
            (
                "unsupported",
                PlatformError::Unsupported {
                    feature: "f".into(),
                },
                "kick.request_failed",
            ),
            (
                "endpoint override",
                PlatformError::EndpointOverrideRefused {
                    variable: "V",
                    reason: forge_platform_core::EndpointRefusal::NotLoopback,
                },
                "kick.request_failed",
            ),
        ];
        for (case, error, code) in cases {
            assert_eq!(failed(&error).code_or_text(), code, "{case}");
        }
    }

    #[test]
    fn upstream_text_travels_only_as_detail_never_as_an_argument() {
        let errors = [
            PlatformError::Auth {
                reason: UPSTREAM.into(),
            },
            PlatformError::MalformedResponse {
                reason: UPSTREAM.into(),
            },
            PlatformError::Network {
                reason: UPSTREAM.into(),
            },
            PlatformError::Refused {
                reason: UPSTREAM.into(),
            },
            io_error(),
        ];
        for error in errors {
            let text = failed(&error);
            let message = message(&text);
            assert!(
                message
                    .detail()
                    .is_some_and(|d| d.as_str().contains(UPSTREAM)),
                "{error:?}"
            );
            assert!(
                message
                    .args()
                    .all(|(_, v)| !format!("{v:?}").contains(UPSTREAM)),
                "{error:?}"
            );
            assert!(!message.code().to_string().contains(UPSTREAM));
        }
    }

    #[test]
    fn http_refusal_drops_the_upstream_body_entirely() {
        let text = failed(&PlatformError::Http {
            status: 500,
            body: UPSTREAM.into(),
        });
        let message = message(&text);

        assert!(message.detail().is_none());
        assert!(
            message
                .args()
                .all(|(_, v)| !format!("{v:?}").contains(UPSTREAM))
        );
    }

    #[test]
    fn http_refusal_carries_the_status_as_an_integer_argument() {
        let text = failed(&PlatformError::Http {
            status: 429,
            body: String::new(),
        });

        assert_eq!(message(&text).arg("status"), Some(&MessageArg::Int(429)));
    }

    #[test]
    fn rate_limit_carries_the_retry_delay_and_no_detail() {
        let text = failed(&PlatformError::RateLimited {
            retry_after_secs: 42,
        });

        assert_eq!(message(&text).arg("seconds"), Some(&MessageArg::Int(42)));
        assert!(message(&text).detail().is_none());
    }

    #[test]
    fn reauth_text_does_not_leak_the_platform_error_display() {
        let text = failed(&PlatformError::ReauthRequired {
            platform: UPSTREAM.into(),
        });

        let message = message(&text);
        assert!(message.detail().is_none());
        assert_eq!(message.args().count(), 0);
    }

    #[test]
    fn outcome_is_success_for_ok_and_a_coded_failure_for_err() {
        assert_eq!(outcome(&Ok::<(), _>(())), SubActionOutcome::Success);
        assert!(matches!(
            outcome(&Err::<(), _>(PlatformError::RateLimitExhausted)),
            SubActionOutcome::Failed(reason)
                if reason.code_or_text() == "kick.rate_budget_exhausted"
        ));
    }

    #[test]
    fn token_and_broadcaster_failures_use_their_own_codes_with_the_cause_as_detail() {
        let cause = PlatformError::Network {
            reason: UPSTREAM.into(),
        };

        let token = token_failed(&cause);
        let broadcaster = broadcaster_failed(&cause);

        assert_eq!(token.code_or_text(), "kick.token_unavailable");
        assert_eq!(broadcaster.code_or_text(), "kick.broadcaster_unavailable");
        for text in [&token, &broadcaster] {
            assert!(
                message(text)
                    .detail()
                    .is_some_and(|d| d.as_str().contains(UPSTREAM))
            );
            assert_eq!(message(text).args().count(), 0);
        }
    }

    #[test]
    fn reauth_during_token_or_broadcaster_lookup_asks_for_sign_in_again() {
        let reauth = || PlatformError::ReauthRequired {
            platform: "kick".into(),
        };

        assert_eq!(
            token_failed(&reauth()).code_or_text(),
            "kick.reauth_required"
        );
        assert_eq!(
            broadcaster_failed(&reauth()).code_or_text(),
            "kick.reauth_required"
        );
    }
}
