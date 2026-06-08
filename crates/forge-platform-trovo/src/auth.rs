use std::time::{Duration, SystemTime};

use forge_platform_core::auth::LocalCallbackDriver;
use forge_platform_core::{AuthFlow, PlatformError};
use forge_types::OAuthToken;
use serde::Deserialize;
use thiserror::Error;

pub const TROVO_AUTHORIZE_ENDPOINT: &str = "https://open.trovo.live/page/login.html";
pub const TROVO_TOKEN_ENDPOINT: &str = "https://open-api.trovo.live/openplatform/exchangetoken";
pub const TROVO_REFRESH_ENDPOINT: &str = "https://open-api.trovo.live/openplatform/refreshtoken";
pub const TROVO_USER_INFO_ENDPOINT: &str = "https://open-api.trovo.live/openplatform/getuserinfo";

pub const TROVO_BROADCASTER_SCOPES: &[&str] = &[
    "user_details_self",
    "channel_details_self",
    "chat_connect",
    "send_to_my_channel",
];

const CALLBACK_REDIRECT_PATH: &str = "/oauth/callback";

pub fn trovo_auth_flow() -> AuthFlow {
    AuthFlow::LocalCallback {
        authorize_url: TROVO_AUTHORIZE_ENDPOINT.to_owned(),
        token_endpoint: TROVO_TOKEN_ENDPOINT.to_owned(),
        redirect_path: CALLBACK_REDIRECT_PATH.to_owned(),
        scopes: TROVO_BROADCASTER_SCOPES
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

pub fn client_credentials() -> Option<(String, String)> {
    let id = option_env!("FORGE_TROVO_CLIENT_ID")?;
    let secret = option_env!("FORGE_TROVO_CLIENT_SECRET")?;
    if id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((id.to_owned(), secret.to_owned()))
}

#[derive(Debug, Clone)]
pub struct LoopbackCode {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub struct TrovoAuthBundle {
    pub access_token: OAuthToken,
    pub refresh_token: OAuthToken,
    pub username: String,
    pub user_id: String,
    pub client_id: String,
    pub expires_at: SystemTime,
}

#[derive(Debug, Error)]
pub enum TrovoAuthError {
    #[error("HTTP error {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("wait_for_authorization called before start")]
    NotStarted,
    #[error("loopback callback failure: {0}")]
    Loopback(#[from] PlatformError),
}

pub struct TrovoAuthFlow {
    client: reqwest::Client,
    client_id: String,
    client_secret: String,
    authorize_endpoint: String,
    token_endpoint: String,
    user_info_endpoint: String,
    pending: Option<LocalCallbackDriver>,
}

impl TrovoAuthFlow {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self::with_endpoints(
            client_id,
            client_secret,
            TROVO_AUTHORIZE_ENDPOINT.to_owned(),
            TROVO_TOKEN_ENDPOINT.to_owned(),
            TROVO_USER_INFO_ENDPOINT.to_owned(),
        )
    }

    pub(crate) fn with_endpoints(
        client_id: String,
        client_secret: String,
        authorize_endpoint: String,
        token_endpoint: String,
        user_info_endpoint: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id,
            client_secret,
            authorize_endpoint,
            token_endpoint,
            user_info_endpoint,
            pending: None,
        }
    }

    /// Binds a loopback listener, generates `state` (CSRF), stores the driver, and
    /// returns the URL the caller should open in the user's browser.
    ///
    /// Trovo does not support PKCE; `code_challenge` is omitted from the authorize URL.
    /// `state` is included for CSRF protection.
    pub async fn start(&mut self) -> Result<LoopbackCode, TrovoAuthError> {
        let driver = LocalCallbackDriver::bind().await?;
        let auth_url = build_authorize_url(&self.authorize_endpoint, &self.client_id, &driver)?;
        self.pending = Some(driver);
        Ok(LoopbackCode { auth_url })
    }

    /// Consumes the pending driver, awaits the loopback callback, exchanges the
    /// authorization code for tokens (JSON POST, `client-id` in header), and
    /// resolves the authenticated user via `getuserinfo`.
    pub async fn wait_for_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<TrovoAuthBundle, TrovoAuthError> {
        let driver = self.pending.take().ok_or(TrovoAuthError::NotStarted)?;
        let redirect_uri = driver.redirect_uri().to_owned();
        let callback = driver.await_callback(timeout).await?;

        let response = self
            .client
            .post(&self.token_endpoint)
            .header("client-id", self.client_id.as_str())
            .json(&serde_json::json!({
                "client_secret": self.client_secret,
                "grant_type": "authorization_code",
                "code": callback.code,
                "redirect_uri": redirect_uri,
            }))
            .send()
            .await
            .map_err(|e| TrovoAuthError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(TrovoAuthError::Http { status, body });
        }

        let token: TokenSuccessResponse = response
            .json()
            .await
            .map_err(|e| TrovoAuthError::Network(e.to_string()))?;

        self.resolve_user(token.access_token, token.refresh_token, token.expires_in)
            .await
    }

    async fn resolve_user(
        &self,
        access_token: String,
        refresh_token: String,
        expires_in: u64,
    ) -> Result<TrovoAuthBundle, TrovoAuthError> {
        let response = self
            .client
            .get(&self.user_info_endpoint)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {access_token}"),
            )
            .header("client-id", self.client_id.as_str())
            .send()
            .await
            .map_err(|e| TrovoAuthError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(TrovoAuthError::Http { status, body });
        }

        let user: UserInfoResponse = response
            .json()
            .await
            .map_err(|e| TrovoAuthError::Network(e.to_string()))?;

        Ok(TrovoAuthBundle {
            access_token: OAuthToken::new(access_token),
            refresh_token: OAuthToken::new(refresh_token),
            username: user.user_name,
            user_id: user.user_id,
            client_id: self.client_id.clone(),
            expires_at: SystemTime::now() + Duration::from_secs(expires_in),
        })
    }
}

fn build_authorize_url(
    endpoint: &str,
    client_id: &str,
    driver: &LocalCallbackDriver,
) -> Result<String, TrovoAuthError> {
    let scope_str = TROVO_BROADCASTER_SCOPES.join(" ");
    let params = [
        ("client_id", client_id),
        ("response_type", "code"),
        ("scope", scope_str.as_str()),
        ("redirect_uri", driver.redirect_uri()),
        ("state", driver.state()),
    ];
    let url = reqwest::Url::parse_with_params(endpoint, &params)
        .map_err(|e| TrovoAuthError::Network(format!("invalid authorize endpoint URL: {e}")))?;
    Ok(url.into())
}

#[derive(Deserialize)]
struct TokenSuccessResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct UserInfoResponse {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "userName")]
    user_name: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_platform_core::AuthFlow;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn token_success_body() -> serde_json::Value {
        json!({
            "access_token": "trovo_access_123",
            "refresh_token": "trovo_refresh_456",
            "expires_in": 86400,
            "token_type": "bearer"
        })
    }

    fn user_info_body() -> serde_json::Value {
        json!({
            "userId": "uid_42",
            "userName": "test_streamer",
            "nickName": "TestStreamer"
        })
    }

    async fn mount_user_info_mock(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/getuserinfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(user_info_body()))
            .mount(server)
            .await;
    }

    #[test]
    fn trovo_auth_flow_returns_local_callback_variant() {
        let flow = trovo_auth_flow();
        let AuthFlow::LocalCallback {
            authorize_url,
            token_endpoint,
            redirect_path,
            scopes,
        } = flow
        else {
            unreachable!("trovo_auth_flow must return LocalCallback variant");
        };
        assert_eq!(authorize_url, TROVO_AUTHORIZE_ENDPOINT);
        assert_eq!(token_endpoint, TROVO_TOKEN_ENDPOINT);
        assert_eq!(redirect_path, CALLBACK_REDIRECT_PATH);
        assert_eq!(
            scopes,
            TROVO_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn scope_list_contains_required_scopes() {
        assert!(TROVO_BROADCASTER_SCOPES.contains(&"chat_connect"));
        assert!(TROVO_BROADCASTER_SCOPES.contains(&"send_to_my_channel"));
        assert!(TROVO_BROADCASTER_SCOPES.contains(&"user_details_self"));
    }

    #[test]
    fn client_credentials_result_is_consistent() {
        let result = client_credentials();
        if let Some((id, secret)) = result {
            assert!(!id.is_empty());
            assert!(!secret.is_empty());
        }
    }

    #[tokio::test]
    async fn authorize_url_contains_required_params_and_omits_pkce() {
        let driver = LocalCallbackDriver::bind().await.unwrap();
        let url = build_authorize_url(TROVO_AUTHORIZE_ENDPOINT, "test_client", &driver).unwrap();
        assert!(url.starts_with(TROVO_AUTHORIZE_ENDPOINT));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=test_client"));
        assert!(url.contains(&format!("state={}", driver.state())));
        assert!(
            !url.contains("code_challenge"),
            "PKCE must be omitted for Trovo"
        );
    }

    #[tokio::test]
    async fn authorize_url_encodes_redirect_uri() {
        let driver = LocalCallbackDriver::bind().await.unwrap();
        let url = build_authorize_url(TROVO_AUTHORIZE_ENDPOINT, "cid", &driver).unwrap();
        assert!(url.contains("redirect_uri="));
        assert!(
            url.contains("127.0.0.1"),
            "redirect_uri must reference loopback address"
        );
    }

    #[tokio::test]
    async fn wait_for_authorization_returns_not_started_when_no_pending() {
        let mut flow = TrovoAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            "http://example.com/auth".to_owned(),
            "http://example.com/token".to_owned(),
            "http://example.com/userinfo".to_owned(),
        );
        let err = flow
            .wait_for_authorization(Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(matches!(err, TrovoAuthError::NotStarted));
    }

    #[tokio::test]
    async fn token_exchange_uses_json_body_with_client_id_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(header("client-id", "cid"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_success_body()))
            .mount(&server)
            .await;
        mount_user_info_mock(&server).await;

        let mut flow = TrovoAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/auth", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/getuserinfo", server.uri()),
        );
        flow.pending = Some(LocalCallbackDriver::bind().await.unwrap());

        let driver = flow.pending.as_ref().unwrap();
        let redirect_uri = driver.redirect_uri().to_owned();
        let state = driver.state().to_owned();
        spawn_callback(&redirect_uri, &state, "code_abc").await;

        let bundle = flow
            .wait_for_authorization(Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(bundle.user_id, "uid_42");
        assert_eq!(bundle.username, "test_streamer");
        assert_eq!(bundle.client_id, "cid");
    }

    #[tokio::test]
    async fn wait_for_authorization_resolves_bundle_fields() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_success_body()))
            .mount(&server)
            .await;
        mount_user_info_mock(&server).await;

        let mut flow = TrovoAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/auth", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/getuserinfo", server.uri()),
        );
        flow.pending = Some(LocalCallbackDriver::bind().await.unwrap());

        let driver = flow.pending.as_ref().unwrap();
        let redirect_uri = driver.redirect_uri().to_owned();
        let state = driver.state().to_owned();
        spawn_callback(&redirect_uri, &state, "auth_code_xyz").await;

        let bundle = flow
            .wait_for_authorization(Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(bundle.user_id, "uid_42");
        assert_eq!(bundle.username, "test_streamer");
        assert_eq!(bundle.client_id, "cid");
        assert!(bundle.expires_at > SystemTime::now());
    }

    #[tokio::test]
    async fn token_exchange_non_200_returns_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error":"invalid_client"})),
            )
            .mount(&server)
            .await;

        let mut flow = TrovoAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/auth", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/getuserinfo", server.uri()),
        );
        flow.pending = Some(LocalCallbackDriver::bind().await.unwrap());

        let driver = flow.pending.as_ref().unwrap();
        let redirect_uri = driver.redirect_uri().to_owned();
        let state = driver.state().to_owned();
        spawn_callback(&redirect_uri, &state, "bad_code").await;

        let err = flow
            .wait_for_authorization(Duration::from_secs(2))
            .await
            .unwrap_err();
        assert!(
            matches!(err, TrovoAuthError::Http { status: 400, .. }),
            "expected Http 400, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn user_info_non_200_returns_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_success_body()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/getuserinfo"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let mut flow = TrovoAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/auth", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/getuserinfo", server.uri()),
        );
        flow.pending = Some(LocalCallbackDriver::bind().await.unwrap());

        let driver = flow.pending.as_ref().unwrap();
        let redirect_uri = driver.redirect_uri().to_owned();
        let state = driver.state().to_owned();
        spawn_callback(&redirect_uri, &state, "code_good").await;

        let err = flow
            .wait_for_authorization(Duration::from_secs(2))
            .await
            .unwrap_err();
        assert!(
            matches!(err, TrovoAuthError::Http { status: 401, .. }),
            "expected Http 401, got: {err:?}"
        );
    }

    async fn spawn_callback(redirect_uri: &str, state: &str, code: &str) {
        let url = format!("{redirect_uri}?code={code}&state={state}");
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = reqwest::Client::new().get(&url).send().await;
        });
    }
}
