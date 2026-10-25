use std::sync::Arc;
use std::time::Duration;

use forge_net::status::BAD_REQUEST;
use forge_net::{
    CallClass, DEFAULT_RETRY_AFTER_SECS, FormBody, Gateway, GatewayBuildError, GatewayFailure,
    GatewayProfile, GatewayRequest, GatewayResponse, RateLimiter, Secret, ThrottleHintFamily,
};
use forge_platform_core::auth::PkceAuthorization;
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError, reauth_required};
use serde::Deserialize;

const PLATFORM: &str = "youtube";
const TOKEN_PATH: &str = "/token";

const AUTHORIZATION_CODE_GRANT_TYPE: &str = "authorization_code";
const REFRESH_GRANT_TYPE: &str = "refresh_token";
const INVALID_GRANT: &str = "invalid_grant";

const NO_RETRY_AFTER_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);

pub const GOOGLE_OAUTH_BUDGET_CAPACITY: u32 = 10;
pub const GOOGLE_OAUTH_BUDGET_WINDOW: Duration = Duration::from_secs(60);

pub const GOOGLE_OAUTH_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::OAuth,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

pub fn google_oauth_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
) -> Result<Gateway, GatewayBuildError> {
    Gateway::new(
        endpoints.base_url(EndpointSurface::YouTubeOAuth),
        limiter,
        GOOGLE_OAUTH_GATEWAY_PROFILE,
        None,
    )
}

#[derive(Clone)]
pub struct GoogleOAuthClient {
    gateway: Gateway,
    client_id: String,
    client_secret: String,
}

#[derive(Deserialize)]
pub(crate) struct TokenGrant {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_in: Option<u64>,
}

#[derive(Deserialize)]
struct OAuthErrorBody {
    error: String,
}

impl GoogleOAuthClient {
    pub fn new(gateway: Gateway, client_id: String, client_secret: String) -> Self {
        Self {
            gateway,
            client_id,
            client_secret,
        }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub(crate) async fn exchange_code(
        &self,
        authorization: &PkceAuthorization,
    ) -> Result<TokenGrant, PlatformError> {
        let form = FormBody::new()
            .field("client_id", self.client_id.as_str())
            .secret_field("client_secret", Secret::new(self.client_secret.as_str()))
            .secret_field("code", Secret::new(authorization.code.as_str()))
            .field("grant_type", AUTHORIZATION_CODE_GRANT_TYPE)
            .field("redirect_uri", authorization.redirect_uri.as_str())
            .secret_field(
                "code_verifier",
                Secret::new(authorization.code_verifier.as_str()),
            );
        let request = GatewayRequest::post(TOKEN_PATH)
            .label(TOKEN_PATH)
            .body(form);
        let response = self.gateway.send(request).await.map_err(oauth_error)?;
        decode(&response)
    }

    pub(crate) async fn refresh(&self, refresh_token: &str) -> Result<TokenGrant, PlatformError> {
        let form = FormBody::new()
            .field("grant_type", REFRESH_GRANT_TYPE)
            .secret_field("refresh_token", Secret::new(refresh_token))
            .field("client_id", self.client_id.as_str())
            .secret_field("client_secret", Secret::new(self.client_secret.as_str()));
        let request = GatewayRequest::post(TOKEN_PATH)
            .label(TOKEN_PATH)
            .body(form);
        match self.gateway.send(request).await {
            Ok(response) => decode(&response),
            Err(failure) if is_invalid_grant(&failure) => Err(reauth_required(PLATFORM)),
            Err(failure) => Err(oauth_error(failure)),
        }
    }
}

fn is_invalid_grant(failure: &GatewayFailure) -> bool {
    let excerpt = match failure {
        GatewayFailure::Unauthorized { excerpt } => excerpt,
        GatewayFailure::ClientRejected { status, excerpt } if *status == BAD_REQUEST => excerpt,
        _ => return false,
    };
    serde_json::from_str::<OAuthErrorBody>(excerpt.as_str())
        .is_ok_and(|body| body.error == INVALID_GRANT)
}

fn oauth_error(failure: GatewayFailure) -> PlatformError {
    match failure {
        GatewayFailure::BudgetExhausted { .. } => PlatformError::RateLimitExhausted,
        GatewayFailure::RateLimited { retry_after, .. } => PlatformError::RateLimited {
            retry_after_secs: u32::try_from(retry_after.as_secs()).unwrap_or(u32::MAX),
        },
        other => match (other.status(), other.excerpt()) {
            (Some(status), Some(excerpt)) => PlatformError::Http {
                status,
                body: excerpt.as_str().to_owned(),
            },
            _ => PlatformError::Network {
                reason: other.to_string(),
            },
        },
    }
}

fn decode(response: &GatewayResponse) -> Result<TokenGrant, PlatformError> {
    response
        .json()
        .map_err(|error| PlatformError::MalformedResponse {
            reason: error.to_string(),
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use forge_net::{RateLimitError, RateLimitOutcome, RateLimiter};
    use forge_platform_core::PlatformError;
    use forge_platform_core::auth::PkceAuthorization;
    use reqwest::StatusCode;
    use serde_json::json;

    use super::TOKEN_PATH;
    use crate::endpoints_test_support::{
        OAUTH_CLIENT_ID, OAUTH_CLIENT_SECRET, StubReply, StubServer, oauth_client_at,
        oauth_client_limited_by,
    };

    const REFRESH_TOKEN: &str = "1//refresh-token-secret";
    const CODE: &str = "4/auth-code-secret";
    const CODE_VERIFIER: &str = "code-verifier-secret-0123456789";
    const NO_HINT_RETRY_AFTER_SECS: u32 = 30;

    enum Mapped {
        Reauth,
        Http(StatusCode),
        RateLimited(u32),
        Malformed,
    }

    fn maps_to(error: &PlatformError, expected: &Mapped) -> bool {
        match (error, expected) {
            (PlatformError::ReauthRequired { platform }, Mapped::Reauth) => platform == "youtube",
            (PlatformError::Http { status, .. }, Mapped::Http(expected)) => {
                *status == expected.as_u16()
            }
            (PlatformError::RateLimited { retry_after_secs }, Mapped::RateLimited(secs)) => {
                retry_after_secs == secs
            }
            (PlatformError::MalformedResponse { .. }, Mapped::Malformed) => true,
            _ => false,
        }
    }

    fn oauth_error(status: StatusCode, error: &str) -> StubReply {
        StubReply::json(status.as_u16(), &json!({ "error": error }))
    }

    fn authorization() -> PkceAuthorization {
        PkceAuthorization {
            code: CODE.to_owned(),
            redirect_uri: "http://127.0.0.1:1/callback".to_owned(),
            code_verifier: CODE_VERIFIER.to_owned(),
        }
    }

    async fn refresh_error_for(reply: StubReply) -> PlatformError {
        let server = StubServer::start().await;
        server.reply_always(TOKEN_PATH, reply);
        match oauth_client_at(&server.uri()).refresh(REFRESH_TOKEN).await {
            Ok(_) => panic!("the refresh must fail"),
            Err(error) => error,
        }
    }

    #[tokio::test]
    async fn refresh_posts_exactly_the_refresh_grant_form() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path(TOKEN_PATH))
            .respond_with(
                wiremock::ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_json(
                    json!({ "access_token": "ya29.new-access", "expires_in": 3599 }),
                ),
            )
            .mount(&server)
            .await;

        oauth_client_at(&server.uri())
            .refresh(REFRESH_TOKEN)
            .await
            .unwrap();

        let requests = server.received_requests().await.unwrap();
        let body = std::str::from_utf8(&requests[0].body).unwrap();
        let sent: std::collections::BTreeMap<String, String> =
            reqwest::Url::parse(&format!("http://form.invalid/?{body}"))
                .unwrap()
                .query_pairs()
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect();
        let expected: std::collections::BTreeMap<String, String> = [
            ("client_id", OAUTH_CLIENT_ID),
            ("client_secret", OAUTH_CLIENT_SECRET),
            ("grant_type", "refresh_token"),
            ("refresh_token", REFRESH_TOKEN),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
        assert_eq!((requests.len(), sent), (1, expected));
    }

    #[tokio::test]
    async fn refresh_maps_each_token_endpoint_answer_to_its_platform_error() {
        for (label, reply, expected) in [
            (
                "400 invalid_grant",
                oauth_error(StatusCode::BAD_REQUEST, "invalid_grant"),
                Mapped::Reauth,
            ),
            (
                "401 invalid_grant",
                oauth_error(StatusCode::UNAUTHORIZED, "invalid_grant"),
                Mapped::Reauth,
            ),
            (
                "401 invalid_client",
                oauth_error(StatusCode::UNAUTHORIZED, "invalid_client"),
                Mapped::Http(StatusCode::UNAUTHORIZED),
            ),
            (
                "400 invalid_request",
                oauth_error(StatusCode::BAD_REQUEST, "invalid_request"),
                Mapped::Http(StatusCode::BAD_REQUEST),
            ),
            (
                "403 invalid_grant",
                oauth_error(StatusCode::FORBIDDEN, "invalid_grant"),
                Mapped::Http(StatusCode::FORBIDDEN),
            ),
            (
                "400 invalid_grant as plain text",
                StubReply::text(StatusCode::BAD_REQUEST.as_u16(), "invalid_grant"),
                Mapped::Http(StatusCode::BAD_REQUEST),
            ),
            (
                "429 without a hint",
                StubReply::status(StatusCode::TOO_MANY_REQUESTS.as_u16()),
                Mapped::RateLimited(NO_HINT_RETRY_AFTER_SECS),
            ),
            (
                "429 with Retry-After",
                StubReply::status(StatusCode::TOO_MANY_REQUESTS.as_u16())
                    .header("retry-after", "12"),
                Mapped::RateLimited(12),
            ),
            (
                "500",
                StubReply::status(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
                Mapped::Http(StatusCode::INTERNAL_SERVER_ERROR),
            ),
            (
                "undecodable 200",
                StubReply::text(StatusCode::OK.as_u16(), "<html>not a token</html>"),
                Mapped::Malformed,
            ),
        ] {
            let error = refresh_error_for(reply).await;
            assert!(maps_to(&error, &expected), "{label}: {error:?}");
        }
    }

    #[tokio::test]
    async fn an_invalid_grant_on_the_code_exchange_is_not_a_reauth() {
        let server = StubServer::start().await;
        server.reply_always(
            TOKEN_PATH,
            oauth_error(StatusCode::BAD_REQUEST, "invalid_grant"),
        );

        let outcome = oauth_client_at(&server.uri())
            .exchange_code(&authorization())
            .await;

        assert!(
            matches!(outcome, Err(PlatformError::Http { status, .. }) if status == StatusCode::BAD_REQUEST.as_u16()),
            "{:?}",
            outcome.err()
        );
    }

    struct SpentBudget;

    #[async_trait]
    impl RateLimiter for SpentBudget {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, RateLimitError> {
            Ok(RateLimitOutcome::Exhausted)
        }

        async fn observe_remote_throttle(&self, _retry_after: std::time::Duration) {}
    }

    #[tokio::test]
    async fn an_exhausted_budget_refuses_both_grants_without_a_request() {
        let server = StubServer::start().await;
        let client = oauth_client_limited_by(&server.uri(), Arc::new(SpentBudget));

        let refresh = client.refresh(REFRESH_TOKEN).await.err();
        let exchange = client.exchange_code(&authorization()).await.err();

        assert!(
            matches!(refresh, Some(PlatformError::RateLimitExhausted))
                && matches!(exchange, Some(PlatformError::RateLimitExhausted))
                && server.request_count(TOKEN_PATH) == 0,
            "{refresh:?} / {exchange:?}"
        );
    }

    #[tokio::test]
    async fn an_unreachable_token_endpoint_maps_to_network_without_the_url() {
        let host = "127.0.0.1:1";

        let error = oauth_client_at(&format!("http://{host}"))
            .refresh(REFRESH_TOKEN)
            .await
            .err();

        let Some(PlatformError::Network { reason }) = &error else {
            panic!("expected Network, got {error:?}");
        };
        assert!(
            !reason.contains(host) && !reason.contains(REFRESH_TOKEN),
            "{reason}"
        );
    }

    async fn error_echoing(
        status: StatusCode,
        sent: &[&str],
        call: impl AsyncFnOnce(&super::GoogleOAuthClient) -> Result<super::TokenGrant, PlatformError>,
    ) -> PlatformError {
        let server = StubServer::start().await;
        server.reply_always(
            TOKEN_PATH,
            StubReply::json(
                status.as_u16(),
                &json!({ "error": "x", "echo": sent.join(" ") }),
            ),
        );
        match call(&oauth_client_at(&server.uri())).await {
            Ok(_) => panic!("the grant must fail"),
            Err(error) => error,
        }
    }

    #[tokio::test]
    async fn secrets_sent_to_the_token_endpoint_never_reach_an_error() {
        let refresh_secrets = [OAUTH_CLIENT_SECRET, REFRESH_TOKEN];
        let exchange_secrets = [OAUTH_CLIENT_SECRET, CODE, CODE_VERIFIER];
        for status in [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let refused_refresh = error_echoing(status, &refresh_secrets, async |client| {
                client.refresh(REFRESH_TOKEN).await
            })
            .await;
            let refused_exchange = error_echoing(status, &exchange_secrets, async |client| {
                client.exchange_code(&authorization()).await
            })
            .await;

            for (error, secrets) in [
                (refused_refresh, refresh_secrets.as_slice()),
                (refused_exchange, exchange_secrets.as_slice()),
            ] {
                for surface in [error.to_string(), format!("{error:?}")] {
                    for secret in secrets {
                        assert!(!surface.contains(secret), "{status}: {surface}");
                    }
                }
            }
        }
    }
}
