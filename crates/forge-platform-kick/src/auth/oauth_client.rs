use std::sync::Arc;
use std::time::Duration;

use forge_net::{
    CallClass, DEFAULT_RETRY_AFTER_SECS, FormBody, Gateway, GatewayBuildError, GatewayFailure,
    GatewayProfile, GatewayRequest, GatewayResponse, RateLimiter, ThrottleHintFamily,
};
use forge_platform_core::PkceAuthorization;
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError, reauth_required};
use forge_types::{OAuthToken, Secret};
use serde::Deserialize;

use crate::gateway::{gateway_error, malformed};
use forge_net::status::BAD_REQUEST;

const PLATFORM: &str = "kick";
const AUTHORIZE_PATH: &str = "/authorize";
const TOKEN_PATH: &str = "/token";

const AUTHORIZATION_CODE_GRANT_TYPE: &str = "authorization_code";
const REFRESH_GRANT_TYPE: &str = "refresh_token";

const NO_RETRY_AFTER_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);

pub const KICK_OAUTH_BUDGET_CAPACITY: u32 = 10;
pub const KICK_OAUTH_BUDGET_WINDOW: Duration = Duration::from_secs(60);

pub(crate) const KICK_OAUTH_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::OAuth,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

pub fn kick_oauth_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
) -> Result<Gateway, GatewayBuildError> {
    Gateway::new(
        endpoints.base_url(EndpointSurface::KickOAuth),
        limiter,
        KICK_OAUTH_GATEWAY_PROFILE,
        None,
    )
}

#[derive(Clone)]
pub struct KickOAuthClient {
    gateway: Gateway,
    authorize_endpoint: String,
    client_id: String,
    client_secret: Secret,
}

#[derive(Deserialize)]
pub(crate) struct TokenGrant {
    pub(crate) access_token: OAuthToken,
    pub(crate) refresh_token: Option<OAuthToken>,
    pub(crate) expires_in: Option<u64>,
}

impl KickOAuthClient {
    pub fn new(
        endpoints: &PlatformEndpoints,
        gateway: Gateway,
        client_id: String,
        client_secret: Secret,
    ) -> Self {
        Self {
            gateway,
            authorize_endpoint: format!(
                "{}{AUTHORIZE_PATH}",
                endpoints.base_url(EndpointSurface::KickOAuth)
            ),
            client_id,
            client_secret,
        }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub(crate) fn authorize_endpoint(&self) -> &str {
        &self.authorize_endpoint
    }

    pub(crate) async fn exchange_code(
        &self,
        authorization: &PkceAuthorization,
    ) -> Result<TokenGrant, PlatformError> {
        let form = FormBody::new()
            .field("client_id", self.client_id.as_str())
            .secret_field("code", authorization.code.clone())
            .field("grant_type", AUTHORIZATION_CODE_GRANT_TYPE)
            .field("redirect_uri", authorization.redirect_uri.as_str())
            .secret_field("code_verifier", authorization.code_verifier.clone())
            .secret_field("client_secret", self.client_secret.clone());
        let request = GatewayRequest::post(TOKEN_PATH)
            .label(TOKEN_PATH)
            .body(form);
        let response = self.gateway.send(request).await.map_err(gateway_error)?;
        decode(&response)
    }

    pub(crate) async fn refresh(&self, refresh_token: &str) -> Result<TokenGrant, PlatformError> {
        let form = FormBody::new()
            .field("grant_type", REFRESH_GRANT_TYPE)
            .secret_field("refresh_token", Secret::new(refresh_token))
            .field("client_id", self.client_id.as_str())
            .secret_field("client_secret", self.client_secret.clone());
        let request = GatewayRequest::post(TOKEN_PATH)
            .label(TOKEN_PATH)
            .body(form);
        match self.gateway.send(request).await {
            Ok(response) => decode(&response),
            Err(failure) if grant_rejected(&failure) => Err(reauth_required(PLATFORM)),
            Err(failure) => Err(gateway_error(failure)),
        }
    }
}

fn grant_rejected(failure: &GatewayFailure) -> bool {
    matches!(
        failure,
        GatewayFailure::Unauthorized { .. }
            | GatewayFailure::ClientRejected {
                status: BAD_REQUEST,
                ..
            }
    )
}

fn decode(response: &GatewayResponse) -> Result<TokenGrant, PlatformError> {
    response.json().map_err(malformed)
}

#[cfg(test)]
mod tests {
    use forge_types::Secret;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use forge_platform_core::PkceAuthorization;
    use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError};
    use reqwest::StatusCode;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::{KickOAuthClient, TOKEN_PATH, TokenGrant};
    use crate::auth::{KICK_AUTHORIZE_ENDPOINT, KICK_TOKEN_ENDPOINT};
    use crate::endpoints_test_support::{
        ExhaustedLimiter, GrantingLimiter, OAUTH_CLIENT_ID, OAUTH_CLIENT_SECRET, endpoints_at,
        oauth_client_at, oauth_client_limited_by, oauth_client_over,
    };

    const REFRESH_TOKEN: &str = "kick-refresh-token-secret-41";
    const CODE: &str = "kick-auth-code-secret-77";
    const CODE_VERIFIER: &str = "kick-code-verifier-secret-0123456789";
    const REDIRECT_URI: &str = "http://127.0.0.1:53127/callback";
    const NO_HINT_RETRY_AFTER_SECS: u32 = 30;

    enum Mapped {
        Reauth,
        Http(StatusCode),
        RateLimited(u32),
        Malformed,
    }

    fn maps_to(error: &PlatformError, expected: &Mapped) -> bool {
        match (error, expected) {
            (PlatformError::ReauthRequired { platform }, Mapped::Reauth) => platform == "kick",
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

    fn authorization() -> PkceAuthorization {
        PkceAuthorization {
            code: Secret::new(CODE),
            redirect_uri: REDIRECT_URI.to_owned(),
            code_verifier: Secret::new(CODE_VERIFIER),
        }
    }

    fn status(code: StatusCode) -> ResponseTemplate {
        ResponseTemplate::new(code.as_u16())
    }

    fn grant() -> ResponseTemplate {
        status(StatusCode::OK).set_body_json(json!({
            "access_token": "kick-new-access",
            "refresh_token": "kick-new-refresh",
            "expires_in": 7200,
            "token_type": "Bearer"
        }))
    }

    async fn token_endpoint_answering(reply: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(TOKEN_PATH))
            .respond_with(reply)
            .mount(&server)
            .await;
        server
    }

    async fn grant_error(
        reply: ResponseTemplate,
        call: impl AsyncFnOnce(&KickOAuthClient) -> Result<TokenGrant, PlatformError>,
    ) -> PlatformError {
        let server = token_endpoint_answering(reply).await;
        match call(&oauth_client_at(&server.uri())).await {
            Ok(_) => panic!("the grant must fail"),
            Err(error) => error,
        }
    }

    async fn sent_form(server: &MockServer) -> BTreeMap<String, String> {
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let body = std::str::from_utf8(&requests[0].body).unwrap();
        reqwest::Url::parse(&format!("http://form.invalid/?{body}"))
            .unwrap()
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    }

    fn form(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn default_endpoints_put_the_authorize_page_and_token_post_on_the_advertised_urls() {
        let defaults = PlatformEndpoints::default();
        let client = KickOAuthClient::new(
            &defaults,
            super::kick_oauth_gateway(&defaults, Arc::new(GrantingLimiter)).unwrap(),
            OAUTH_CLIENT_ID.to_owned(),
            forge_types::Secret::new(OAUTH_CLIENT_SECRET),
        );
        assert_eq!(
            (
                client.authorize_endpoint().to_owned(),
                format!(
                    "{}{TOKEN_PATH}",
                    defaults.base_url(EndpointSurface::KickOAuth)
                )
            ),
            (
                KICK_AUTHORIZE_ENDPOINT.to_owned(),
                KICK_TOKEN_ENDPOINT.to_owned()
            )
        );
    }

    #[tokio::test]
    async fn code_exchange_posts_the_pkce_form_and_decodes_the_grant() {
        let server = token_endpoint_answering(grant()).await;

        let grant = oauth_client_at(&server.uri())
            .exchange_code(&authorization())
            .await
            .unwrap();

        assert_eq!(
            (
                sent_form(&server).await,
                grant.access_token.expose(),
                grant
                    .refresh_token
                    .as_ref()
                    .map(forge_types::OAuthToken::expose),
                grant.expires_in
            ),
            (
                form(&[
                    ("client_id", OAUTH_CLIENT_ID),
                    ("client_secret", OAUTH_CLIENT_SECRET),
                    ("code", CODE),
                    ("code_verifier", CODE_VERIFIER),
                    ("grant_type", "authorization_code"),
                    ("redirect_uri", REDIRECT_URI),
                ]),
                "kick-new-access",
                Some("kick-new-refresh"),
                Some(7200)
            )
        );
    }

    #[tokio::test]
    async fn refresh_posts_the_refresh_grant_form() {
        let server = token_endpoint_answering(grant()).await;

        oauth_client_at(&server.uri())
            .refresh(REFRESH_TOKEN)
            .await
            .unwrap();

        assert_eq!(
            sent_form(&server).await,
            form(&[
                ("client_id", OAUTH_CLIENT_ID),
                ("client_secret", OAUTH_CLIENT_SECRET),
                ("grant_type", "refresh_token"),
                ("refresh_token", REFRESH_TOKEN),
            ])
        );
    }

    #[tokio::test]
    async fn refresh_maps_each_token_endpoint_answer_to_its_platform_error() {
        for (label, reply, expected) in [
            (
                "400 documented Invalid request",
                status(StatusCode::BAD_REQUEST).set_body_json(json!({"error": "Invalid request"})),
                Mapped::Reauth,
            ),
            (
                "400 invalid_grant",
                status(StatusCode::BAD_REQUEST).set_body_json(json!({"error": "invalid_grant"})),
                Mapped::Reauth,
            ),
            (
                "400 plain text",
                status(StatusCode::BAD_REQUEST).set_body_string("nope"),
                Mapped::Reauth,
            ),
            (
                "401",
                status(StatusCode::UNAUTHORIZED).set_body_string("unauthorized"),
                Mapped::Reauth,
            ),
            (
                "403",
                status(StatusCode::FORBIDDEN),
                Mapped::Http(StatusCode::FORBIDDEN),
            ),
            (
                "429 without a hint",
                status(StatusCode::TOO_MANY_REQUESTS),
                Mapped::RateLimited(NO_HINT_RETRY_AFTER_SECS),
            ),
            (
                "429 with Retry-After",
                status(StatusCode::TOO_MANY_REQUESTS).insert_header("retry-after", "12"),
                Mapped::RateLimited(12),
            ),
            (
                "500",
                status(StatusCode::INTERNAL_SERVER_ERROR),
                Mapped::Http(StatusCode::INTERNAL_SERVER_ERROR),
            ),
            (
                "undecodable 200",
                status(StatusCode::OK).set_body_string("<html>not a token</html>"),
                Mapped::Malformed,
            ),
            (
                "200 without access_token",
                status(StatusCode::OK).set_body_json(json!({"expires_in": 3600})),
                Mapped::Malformed,
            ),
        ] {
            let error =
                grant_error(reply, async |client| client.refresh(REFRESH_TOKEN).await).await;
            assert!(maps_to(&error, &expected), "{label}: {error:?}");
        }
    }

    #[tokio::test]
    async fn code_exchange_maps_each_token_endpoint_answer_to_its_platform_error() {
        for (label, reply, expected) in [
            (
                "400 is a refused code, not a reauth",
                status(StatusCode::BAD_REQUEST).set_body_json(json!({"error": "Invalid request"})),
                Mapped::Http(StatusCode::BAD_REQUEST),
            ),
            (
                "401",
                status(StatusCode::UNAUTHORIZED),
                Mapped::Http(StatusCode::UNAUTHORIZED),
            ),
            (
                "429 without a hint",
                status(StatusCode::TOO_MANY_REQUESTS),
                Mapped::RateLimited(NO_HINT_RETRY_AFTER_SECS),
            ),
            (
                "undecodable 200",
                status(StatusCode::OK).set_body_string("<html>not a token</html>"),
                Mapped::Malformed,
            ),
        ] {
            let error = grant_error(reply, async |client| {
                client.exchange_code(&authorization()).await
            })
            .await;
            assert!(maps_to(&error, &expected), "{label}: {error:?}");
        }
    }

    #[tokio::test]
    async fn an_exhausted_budget_refuses_both_grants_without_a_request() {
        let server = token_endpoint_answering(grant()).await;
        let client = oauth_client_limited_by(&server.uri(), Arc::new(ExhaustedLimiter));

        let refresh = client.refresh(REFRESH_TOKEN).await.err();
        let exchange = client.exchange_code(&authorization()).await.err();

        assert!(
            matches!(refresh, Some(PlatformError::RateLimitExhausted))
                && matches!(exchange, Some(PlatformError::RateLimitExhausted))
                && server.received_requests().await.unwrap().is_empty(),
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

    #[tokio::test]
    async fn secrets_sent_to_the_token_endpoint_never_reach_an_error() {
        let refresh_secrets = [OAUTH_CLIENT_SECRET, REFRESH_TOKEN];
        let exchange_secrets = [OAUTH_CLIENT_SECRET, CODE, CODE_VERIFIER];
        for code in [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            let echo = |secrets: &[&str]| {
                status(code).set_body_json(json!({ "error": "x", "echo": secrets.join(" ") }))
            };
            let refused_refresh = grant_error(echo(&refresh_secrets), async |client| {
                client.refresh(REFRESH_TOKEN).await
            })
            .await;
            let refused_exchange = grant_error(echo(&exchange_secrets), async |client| {
                client.exchange_code(&authorization()).await
            })
            .await;

            for (error, secrets) in [
                (refused_refresh, refresh_secrets.as_slice()),
                (refused_exchange, exchange_secrets.as_slice()),
            ] {
                for surface in [error.to_string(), format!("{error:?}")] {
                    for secret in secrets {
                        assert!(!surface.contains(secret), "{code}: {surface}");
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn token_grants_go_to_the_oauth_override_and_never_the_api_host() {
        let oauth = token_endpoint_answering(grant()).await;
        let api = token_endpoint_answering(grant()).await;
        let endpoints = endpoints_at(&[
            (EndpointSurface::KickOAuth, &oauth.uri()),
            (EndpointSurface::KickPublicApi, &api.uri()),
        ]);
        let client = oauth_client_over(&endpoints, Arc::new(GrantingLimiter));

        client.refresh(REFRESH_TOKEN).await.unwrap();
        client.exchange_code(&authorization()).await.unwrap();

        assert_eq!(
            (
                oauth.received_requests().await.unwrap().len(),
                api.received_requests().await.unwrap().len()
            ),
            (2, 0)
        );
    }
}
