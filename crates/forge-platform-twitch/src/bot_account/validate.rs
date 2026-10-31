use forge_platform_core::PlatformError;
use forge_types::OAuthToken;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::auth::TWITCH_BOT_SCOPES;
use crate::oauth_client::{TwitchOAuthClient, ValidatedToken};
use forge_net::status::UNAUTHORIZED;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenVerdict {
    Valid,
    Invalid,
    Unknown,
}

pub(super) struct TokenValidator {
    oauth: TwitchOAuthClient,
}

impl TokenValidator {
    pub(super) fn new(oauth: TwitchOAuthClient) -> Self {
        Self { oauth }
    }

    pub(super) async fn check(&self, token: &OAuthToken) -> TokenVerdict {
        match self.oauth.validate(token, &CancellationToken::new()).await {
            Ok(validated) => verdict_for(&validated, self.oauth.client_id()),
            Err(PlatformError::Http { status, .. }) if status == UNAUTHORIZED => {
                TokenVerdict::Invalid
            }
            Err(e) => {
                debug!(error = %e, "bot token validation inconclusive");
                TokenVerdict::Unknown
            }
        }
    }
}

fn verdict_for(validated: &ValidatedToken, client_id: &str) -> TokenVerdict {
    let scopes_complete = TWITCH_BOT_SCOPES
        .iter()
        .all(|required| validated.scopes.iter().any(|granted| granted == required));
    if validated.client_id == client_id && scopes_complete {
        TokenVerdict::Valid
    } else {
        TokenVerdict::Invalid
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_net::RateLimiter;
    use forge_net::test_support::RecordingRateLimiter;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::sub_actions::test_support::oauth_client_limited_by;

    const CLIENT: &str = "test_client";
    const TOKEN: &str = "BOT_VALIDATE_SENTINEL_q8";
    const UNREACHABLE: &str = "http://127.0.0.1:0";

    enum Answer {
        Http(ResponseTemplate),
        BudgetExhausted,
        Unreachable,
    }

    fn validated(client_id: &str, scopes: &[&str]) -> Answer {
        Answer::Http(
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
                "client_id": client_id,
                "user_id": "4242",
                "scopes": scopes,
                "expires_in": 3600,
            })),
        )
    }

    fn status(code: u16) -> Answer {
        Answer::Http(ResponseTemplate::new(code))
    }

    async fn check_against(answer: Answer, limiter: Arc<RecordingRateLimiter>) -> TokenVerdict {
        let server = MockServer::start().await;
        let base = match answer {
            Answer::Http(response) => {
                Mock::given(path("/validate"))
                    .respond_with(response)
                    .mount(&server)
                    .await;
                server.uri()
            }
            Answer::BudgetExhausted => server.uri(),
            Answer::Unreachable => UNREACHABLE.to_owned(),
        };
        let validator = TokenValidator::new(oauth_client_limited_by(
            &base,
            CLIENT,
            limiter as Arc<dyn RateLimiter>,
        ));
        validator.check(&OAuthToken::new(TOKEN)).await
    }

    #[tokio::test]
    async fn check_classifies_every_validation_answer_into_a_verdict() {
        let mut with_extra = TWITCH_BOT_SCOPES.to_vec();
        with_extra.push("channel:bot");
        let missing_last = &TWITCH_BOT_SCOPES[..TWITCH_BOT_SCOPES.len() - 1];
        let cases = [
            (
                "all bot scopes",
                validated(CLIENT, TWITCH_BOT_SCOPES),
                TokenVerdict::Valid,
            ),
            (
                "extra scopes",
                validated(CLIENT, &with_extra),
                TokenVerdict::Valid,
            ),
            (
                "foreign client",
                validated("other_client", TWITCH_BOT_SCOPES),
                TokenVerdict::Invalid,
            ),
            (
                "missing scope",
                validated(CLIENT, missing_last),
                TokenVerdict::Invalid,
            ),
            (
                "no scopes field",
                Answer::Http(
                    ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(
                        serde_json::json!({ "client_id": CLIENT, "user_id": "4242" }),
                    ),
                ),
                TokenVerdict::Invalid,
            ),
            ("rejected", status(401), TokenVerdict::Invalid),
            (
                "throttled",
                Answer::Http(
                    ResponseTemplate::new(reqwest::StatusCode::TOO_MANY_REQUESTS)
                        .insert_header("Retry-After", "7"),
                ),
                TokenVerdict::Unknown,
            ),
            (
                "budget exhausted",
                Answer::BudgetExhausted,
                TokenVerdict::Unknown,
            ),
            ("forbidden", status(403), TokenVerdict::Unknown),
            ("server error", status(503), TokenVerdict::Unknown),
            ("unreachable", Answer::Unreachable, TokenVerdict::Unknown),
            (
                "unparseable body",
                Answer::Http(
                    ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("not json"),
                ),
                TokenVerdict::Unknown,
            ),
        ];

        for (label, answer, expected) in cases {
            let limiter = match answer {
                Answer::BudgetExhausted => RecordingRateLimiter::exhausted(),
                _ => RecordingRateLimiter::granting(),
            };

            let verdict = check_against(answer, Arc::new(limiter)).await;

            assert_eq!(verdict, expected, "{label}");
        }
    }

    #[tokio::test]
    async fn a_throttled_validation_cools_the_shared_oauth_limiter() {
        let limiter = Arc::new(RecordingRateLimiter::granting());

        check_against(
            Answer::Http(
                ResponseTemplate::new(reqwest::StatusCode::TOO_MANY_REQUESTS)
                    .insert_header("Retry-After", "7"),
            ),
            Arc::clone(&limiter),
        )
        .await;

        assert_eq!(limiter.observed_throttles(), vec![Duration::from_secs(7)]);
    }
}
