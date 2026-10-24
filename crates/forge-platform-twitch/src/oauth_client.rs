use std::sync::Arc;
use std::time::Duration;

use forge_net::{
    CallClass, FormBody, Gateway, GatewayBuildError, GatewayFailure, GatewayProfile,
    GatewayRequest, GatewayResponse, RateLimiter, Secret, ThrottleHintFamily,
};
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError, reauth_required};
use forge_types::OAuthToken;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;

use crate::auth::{DEVICE_CANCELLED_REASON, DEVICE_CODE_USED_REASON, DEVICE_DENIED_REASON};

const PLATFORM: &str = "twitch";
const DEVICE_PATH: &str = "/device";
const TOKEN_PATH: &str = "/token";
const VALIDATE_PATH: &str = "/validate";
const REVOKE_PATH: &str = "/revoke";
const VALIDATE_SCHEME: &str = "OAuth";

const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const REFRESH_GRANT_TYPE: &str = "refresh_token";

const POLL_PENDING: &str = "authorization_pending";
const POLL_SLOW_DOWN: &str = "slow_down";
const POLL_EXPIRED_TOKEN: &str = "expired_token";
const POLL_INVALID_DEVICE_CODE: &str = "invalid device code";
const POLL_ACCESS_DENIED: &str = "access_denied";

const BAD_REQUEST: u16 = reqwest::StatusCode::BAD_REQUEST.as_u16();
const NO_RETRY_AFTER_WAIT: Duration = Duration::from_secs(5);

pub const TWITCH_OAUTH_BUDGET_CAPACITY: u32 = 120;
pub const TWITCH_OAUTH_BUDGET_WINDOW: Duration = Duration::from_secs(60);

pub const TWITCH_OAUTH_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::OAuth,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

pub fn twitch_oauth_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
) -> Result<Gateway, GatewayBuildError> {
    Gateway::new(
        endpoints.base_url(EndpointSurface::TwitchOAuth),
        limiter,
        TWITCH_OAUTH_GATEWAY_PROFILE,
        None,
    )
}

#[derive(Clone)]
pub struct TwitchOAuthClient {
    gateway: Gateway,
    client_id: String,
}

#[derive(Deserialize)]
pub(crate) struct DeviceAuthorization {
    pub(crate) device_code: String,
    pub(crate) user_code: String,
    pub(crate) verification_uri: String,
    pub(crate) expires_in: u64,
    pub(crate) interval: u64,
}

#[derive(Deserialize)]
pub(crate) struct TokenGrant {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_in: Option<u64>,
}

pub(crate) enum DevicePoll {
    Pending,
    SlowDown,
    Throttled { wait: Duration },
    Granted(TokenGrant),
}

#[derive(Deserialize)]
pub(crate) struct ValidatedToken {
    pub(crate) client_id: String,
    pub(crate) user_id: String,
    #[serde(default)]
    pub(crate) scopes: Vec<String>,
    #[serde(default)]
    pub(crate) expires_in: u64,
}

#[derive(Deserialize)]
struct OAuthErrorBody {
    message: String,
}

impl TwitchOAuthClient {
    pub fn new(gateway: Gateway, client_id: String) -> Self {
        Self { gateway, client_id }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub(crate) async fn request_device_code(
        &self,
        scopes: &[&str],
    ) -> Result<DeviceAuthorization, PlatformError> {
        let form = FormBody::new()
            .field("client_id", self.client_id.as_str())
            .field("scopes", scopes.join(" "));
        let request = GatewayRequest::post(DEVICE_PATH)
            .label(DEVICE_PATH)
            .body(form);
        let response = self.gateway.send(request).await.map_err(oauth_error)?;
        decode(&response)
    }

    pub(crate) async fn poll_device_token(
        &self,
        device_code: &str,
        cancel: &CancellationToken,
    ) -> Result<DevicePoll, PlatformError> {
        let form = FormBody::new()
            .field("client_id", self.client_id.as_str())
            .secret_field("device_code", Secret::new(device_code))
            .field("grant_type", DEVICE_GRANT_TYPE);
        let request = GatewayRequest::post(TOKEN_PATH)
            .label(TOKEN_PATH)
            .body(form)
            .cancel_on(cancel.clone());
        let failure = match self.gateway.send(request).await {
            Ok(response) => return decode(&response).map(DevicePoll::Granted),
            Err(GatewayFailure::RateLimited { retry_after, .. }) => {
                return Ok(DevicePoll::Throttled { wait: retry_after });
            }
            Err(GatewayFailure::BudgetExhausted) => {
                return Ok(DevicePoll::Throttled {
                    wait: Duration::ZERO,
                });
            }
            Err(failure) => failure,
        };
        match error_message(&failure).as_deref() {
            Some(POLL_PENDING) => Ok(DevicePoll::Pending),
            Some(POLL_SLOW_DOWN) => Ok(DevicePoll::SlowDown),
            Some(POLL_EXPIRED_TOKEN | POLL_INVALID_DEVICE_CODE) => Err(PlatformError::Auth {
                reason: DEVICE_CODE_USED_REASON.into(),
            }),
            Some(POLL_ACCESS_DENIED) => Err(PlatformError::Auth {
                reason: DEVICE_DENIED_REASON.into(),
            }),
            _ => Err(oauth_error(failure)),
        }
    }

    pub(crate) async fn refresh(
        &self,
        refresh_token: &OAuthToken,
    ) -> Result<TokenGrant, PlatformError> {
        let form = FormBody::new()
            .field("grant_type", REFRESH_GRANT_TYPE)
            .secret_field("refresh_token", Secret::new(refresh_token.expose()))
            .field("client_id", self.client_id.as_str());
        let request = GatewayRequest::post(TOKEN_PATH)
            .label(TOKEN_PATH)
            .body(form);
        match self.gateway.send(request).await {
            Ok(response) => decode(&response),
            Err(GatewayFailure::Unauthorized { .. }) => Err(reauth_required(PLATFORM)),
            Err(GatewayFailure::ClientRejected { status, .. }) if status == BAD_REQUEST => {
                Err(reauth_required(PLATFORM))
            }
            Err(failure) => Err(oauth_error(failure)),
        }
    }

    pub(crate) async fn validate(
        &self,
        access_token: &OAuthToken,
        cancel: &CancellationToken,
    ) -> Result<ValidatedToken, PlatformError> {
        let request = GatewayRequest::get(VALIDATE_PATH)
            .label(VALIDATE_PATH)
            .authorization(VALIDATE_SCHEME, Secret::new(access_token.expose()))
            .cancel_on(cancel.clone());
        let response = self.gateway.send(request).await.map_err(oauth_error)?;
        decode(&response)
    }

    pub(crate) async fn revoke(&self, token: &OAuthToken) -> Result<(), PlatformError> {
        let form = FormBody::new()
            .field("client_id", self.client_id.as_str())
            .secret_field("token", Secret::new(token.expose()));
        let request = GatewayRequest::post(REVOKE_PATH)
            .label(REVOKE_PATH)
            .body(form);
        self.gateway
            .send(request)
            .await
            .map(drop)
            .map_err(oauth_error)
    }
}

pub(crate) fn oauth_error(failure: GatewayFailure) -> PlatformError {
    match failure {
        GatewayFailure::Cancelled => PlatformError::Auth {
            reason: DEVICE_CANCELLED_REASON.into(),
        },
        GatewayFailure::BudgetExhausted => PlatformError::RateLimitExhausted,
        GatewayFailure::RateLimited { retry_after, .. } => PlatformError::RateLimited {
            retry_after_secs: u32::try_from(retry_after.as_secs()).unwrap_or(u32::MAX),
        },
        other => match other.status() {
            Some(status) => PlatformError::Http {
                status,
                body: other
                    .excerpt()
                    .map(|excerpt| excerpt.as_str().to_owned())
                    .unwrap_or_default(),
            },
            None => PlatformError::Network {
                reason: other.to_string(),
            },
        },
    }
}

pub(crate) fn decode<T: DeserializeOwned>(response: &GatewayResponse) -> Result<T, PlatformError> {
    response
        .json()
        .map_err(|error| PlatformError::MalformedResponse {
            reason: error.to_string(),
        })
}

fn error_message(failure: &GatewayFailure) -> Option<String> {
    let excerpt = failure.excerpt()?;
    serde_json::from_str::<OAuthErrorBody>(excerpt.as_str())
        .ok()
        .map(|body| body.message)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use forge_net::RateLimiter;
    use forge_net::test_support::RecordingRateLimiter;
    use forge_platform_core::PlatformError;
    use forge_types::OAuthToken;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{DevicePoll, TwitchOAuthClient};
    use crate::BotLoginError;
    use crate::sub_actions::test_support::{oauth_client_at, oauth_client_limited_by};

    const CLIENT: &str = "test_client";
    const DEVICE_CODE: &str = "DEVICE_CODE_SENTINEL_x9";
    const REFRESH_TOKEN: &str = "REFRESH_TOKEN_SENTINEL_k4";
    const ACCESS_TOKEN: &str = "ACCESS_TOKEN_SENTINEL_m2";

    async fn answering(route: &str, response: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(path(route))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    fn client_for(server: &MockServer) -> TwitchOAuthClient {
        oauth_client_at(&server.uri(), CLIENT)
    }

    async fn poll(server: &MockServer) -> Result<DevicePoll, PlatformError> {
        client_for(server)
            .poll_device_token(DEVICE_CODE, &CancellationToken::new())
            .await
    }

    async fn refresh(client: &TwitchOAuthClient) -> Result<super::TokenGrant, PlatformError> {
        client.refresh(&OAuthToken::new(REFRESH_TOKEN)).await
    }

    fn message(status: u16, message: &str) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_json(serde_json::json!({ "message": message }))
    }

    async fn only_body(server: &MockServer) -> String {
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        String::from_utf8_lossy(&requests[0].body).into_owned()
    }

    #[tokio::test]
    async fn device_poll_grant_posts_the_device_code_urn_grant_without_scopes() {
        let server = answering(
            "/token",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ACCESS_XYZ",
                "refresh_token": "REFRESH_XYZ",
                "expires_in": 3600,
            })),
        )
        .await;

        let Ok(DevicePoll::Granted(grant)) = poll(&server).await else {
            panic!("a 200 token answer must be a grant");
        };

        assert_eq!(
            (grant.access_token.as_str(), grant.refresh_token.as_deref()),
            ("ACCESS_XYZ", Some("REFRESH_XYZ"))
        );
        let body = only_body(&server).await;
        assert!(
            body.contains(&format!("device_code={DEVICE_CODE}")),
            "{body}"
        );
        assert!(body.contains("client_id=test_client"), "{body}");
        assert!(
            body.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"),
            "{body}"
        );
        assert!(!body.contains("scopes"), "{body}");
    }

    #[tokio::test]
    async fn device_poll_pending_and_slow_down_keep_the_poll_alive() {
        for (answer, want_slow_down) in [("authorization_pending", false), ("slow_down", true)] {
            let server = answering("/token", message(400, answer)).await;

            let outcome = poll(&server).await;

            assert!(
                matches!(
                    (outcome, want_slow_down),
                    (Ok(DevicePoll::Pending), false) | (Ok(DevicePoll::SlowDown), true)
                ),
                "{answer} must map to its retry outcome"
            );
        }
    }

    #[tokio::test]
    async fn terminal_device_poll_answers_map_to_typed_bot_login_errors() {
        type Expectation = fn(&BotLoginError) -> bool;
        let cases: [(&str, Expectation); 3] = [
            ("access_denied", |e| matches!(e, BotLoginError::Denied)),
            ("expired_token", |e| matches!(e, BotLoginError::Expired)),
            ("invalid device code", |e| {
                matches!(e, BotLoginError::Expired)
            }),
        ];

        for (answer, expected) in cases {
            let server = answering("/token", message(400, answer)).await;

            let Err(error) = poll(&server).await else {
                panic!("{answer} must end the poll");
            };
            let mapped = BotLoginError::from(error);

            assert!(expected(&mapped), "{answer} mapped to {mapped:?}");
        }
    }

    #[tokio::test]
    async fn unknown_device_poll_answer_carries_status_and_a_masked_excerpt() {
        for (status, body) in [
            (
                400,
                format!("{{\"message\":\"unrecognised {DEVICE_CODE}\"}}"),
            ),
            (418, format!("teapot {DEVICE_CODE}")),
        ] {
            let server = answering(
                "/token",
                ResponseTemplate::new(status).set_body_string(body),
            )
            .await;

            let Err(PlatformError::Http {
                status: got,
                body: excerpt,
            }) = poll(&server).await
            else {
                panic!("HTTP {status}: an unknown poll answer must be an Http error");
            };

            assert_eq!(got, status);
            assert!(!excerpt.contains(DEVICE_CODE), "{excerpt}");
            assert!(excerpt.contains("***"), "{excerpt}");
        }
    }

    #[tokio::test]
    async fn refresh_posts_a_public_client_refresh_grant() {
        let server = answering(
            "/token",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "access_token": "a" })),
        )
        .await;

        refresh(&client_for(&server)).await.unwrap();

        let body = only_body(&server).await;
        for field in [
            "grant_type=refresh_token",
            "client_id=test_client",
            &format!("refresh_token={REFRESH_TOKEN}"),
        ] {
            assert!(body.contains(field), "missing {field}: {body}");
        }
        assert!(!body.contains("client_secret"), "{body}");
    }

    #[tokio::test]
    async fn refresh_failures_map_to_their_platform_errors() {
        type Expectation = fn(&PlatformError) -> bool;
        let cases: [(ResponseTemplate, Expectation); 6] = [
            (ResponseTemplate::new(400), |e| {
                matches!(e, PlatformError::ReauthRequired { .. })
            }),
            (ResponseTemplate::new(401), |e| {
                matches!(e, PlatformError::ReauthRequired { .. })
            }),
            (ResponseTemplate::new(403), |e| {
                matches!(e, PlatformError::Http { status: 403, .. })
            }),
            (ResponseTemplate::new(503), |e| {
                matches!(e, PlatformError::Http { status: 503, .. })
            }),
            (
                ResponseTemplate::new(429).insert_header("Retry-After", "7"),
                |e| {
                    matches!(
                        e,
                        PlatformError::RateLimited {
                            retry_after_secs: 7
                        }
                    )
                },
            ),
            (
                ResponseTemplate::new(200).set_body_string("not json"),
                |e| matches!(e, PlatformError::MalformedResponse { .. }),
            ),
        ];

        for (response, expected) in cases {
            let server = answering("/token", response).await;

            let error = refresh(&client_for(&server)).await.err().unwrap();

            assert!(expected(&error), "unexpected mapping: {error:?}");
        }
    }

    #[tokio::test]
    async fn refresh_failure_excerpt_masks_the_refresh_token() {
        let server = answering(
            "/token",
            ResponseTemplate::new(503).set_body_string(format!("upstream echoed {REFRESH_TOKEN}")),
        )
        .await;

        let Err(PlatformError::Http { body, .. }) = refresh(&client_for(&server)).await else {
            panic!("a 503 without a retry hint must be an Http error");
        };

        assert!(!body.contains(REFRESH_TOKEN), "{body}");
    }

    #[tokio::test]
    async fn throttled_oauth_answer_cools_the_oauth_limiter() {
        let server = answering(
            "/token",
            ResponseTemplate::new(429).insert_header("Retry-After", "7"),
        )
        .await;
        let limiter = Arc::new(RecordingRateLimiter::granting());
        let client = oauth_client_limited_by(
            &server.uri(),
            CLIENT,
            Arc::clone(&limiter) as Arc<dyn RateLimiter>,
        );

        refresh(&client).await.err().unwrap();

        assert_eq!(limiter.observed_throttles(), vec![Duration::from_secs(7)]);
    }

    #[tokio::test]
    async fn exhausted_oauth_budget_fails_before_reaching_the_oauth_host() {
        let server = answering("/token", ResponseTemplate::new(200)).await;
        let client = oauth_client_limited_by(
            &server.uri(),
            CLIENT,
            Arc::new(RecordingRateLimiter::exhausted()),
        );

        let error = refresh(&client).await.err().unwrap();

        assert!(
            matches!(error, PlatformError::RateLimitExhausted),
            "{error:?}"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn transport_failure_maps_to_a_network_error_without_the_url() {
        let client = oauth_client_at("http://127.0.0.1:0", CLIENT);

        let Err(PlatformError::Network { reason }) = refresh(&client).await else {
            panic!("an unreachable OAuth host must be a Network error");
        };

        assert!(!reason.contains("127.0.0.1"), "{reason}");
        assert!(!reason.contains("/token"), "{reason}");
    }

    #[tokio::test]
    async fn validate_presents_the_token_under_the_oauth_scheme() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/validate"))
            .and(header("Authorization", format!("OAuth {ACCESS_TOKEN}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "client_id": CLIENT,
                "user_id": "42",
            })))
            .mount(&server)
            .await;

        let validated = client_for(&server)
            .validate(&OAuthToken::new(ACCESS_TOKEN), &CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(
            (validated.user_id.as_str(), validated.expires_in),
            ("42", 0)
        );
    }

    #[tokio::test]
    async fn throttled_device_polls_report_a_wait_instead_of_failing() {
        let cases = [
            (
                ResponseTemplate::new(429).insert_header("Retry-After", "7"),
                RecordingRateLimiter::granting(),
                Duration::from_secs(7),
                1,
            ),
            (
                ResponseTemplate::new(200),
                RecordingRateLimiter::exhausted(),
                Duration::ZERO,
                0,
            ),
        ];

        for (response, limiter, expected_wait, expected_requests) in cases {
            let server = answering("/token", response).await;
            let client = oauth_client_limited_by(&server.uri(), CLIENT, Arc::new(limiter));

            let outcome = client
                .poll_device_token(DEVICE_CODE, &CancellationToken::new())
                .await;

            let Ok(DevicePoll::Throttled { wait }) = outcome else {
                panic!("expected a throttled poll for {expected_wait:?}");
            };
            assert_eq!(wait, expected_wait);
            assert_eq!(
                server.received_requests().await.unwrap().len(),
                expected_requests
            );
        }
    }

    #[tokio::test]
    async fn revoke_posts_the_token_and_the_client_id_as_form_fields() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/revoke"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        client_for(&server)
            .revoke(&OAuthToken::new(ACCESS_TOKEN))
            .await
            .unwrap();

        let body = only_body(&server).await;
        assert!(body.contains(&format!("token={ACCESS_TOKEN}")), "{body}");
        assert!(body.contains("client_id=test_client"), "{body}");
    }

    #[tokio::test]
    async fn revoke_failure_excerpt_masks_the_token() {
        let server = answering(
            "/revoke",
            ResponseTemplate::new(400).set_body_string(format!("invalid token {ACCESS_TOKEN}")),
        )
        .await;

        let Err(PlatformError::Http { status, body }) = client_for(&server)
            .revoke(&OAuthToken::new(ACCESS_TOKEN))
            .await
        else {
            panic!("a rejected revoke must be an Http error");
        };

        assert_eq!(status, 400);
        assert!(!body.contains(ACCESS_TOKEN), "{body}");
        assert!(body.contains("***"), "{body}");
    }

    #[tokio::test]
    async fn validate_failure_excerpt_never_carries_the_access_token() {
        for (status, body) in [
            (503, format!("upstream saw {ACCESS_TOKEN}")),
            (403, format!("forbidden {ACCESS_TOKEN}")),
            (403, format!("forbidden OAuth {ACCESS_TOKEN}")),
        ] {
            let server = answering(
                "/validate",
                ResponseTemplate::new(status).set_body_string(body),
            )
            .await;

            let Err(PlatformError::Http { body: excerpt, .. }) = client_for(&server)
                .validate(&OAuthToken::new(ACCESS_TOKEN), &CancellationToken::new())
                .await
            else {
                panic!("HTTP {status}: a rejected validate must be an Http error");
            };

            assert!(!excerpt.contains(ACCESS_TOKEN), "HTTP {status}: {excerpt}");
        }
    }
}
