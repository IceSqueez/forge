use std::time::{Duration, SystemTime};

use forge_platform_core::auth::LocalCallbackDriver;
use forge_platform_core::{AuthFlow, PlatformError};
use forge_types::OAuthToken;
use serde::Deserialize;
use thiserror::Error;

pub const GOOGLE_AUTHORIZE_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

const YOUTUBE_CHANNELS_ENDPOINT: &str = "https://www.googleapis.com/youtube/v3/channels";

pub const YOUTUBE_BROADCASTER_SCOPES: &[&str] =
    &["https://www.googleapis.com/auth/youtube.force-ssl"];

const CALLBACK_REDIRECT_PATH: &str = "/oauth/callback";

pub fn youtube_auth_flow() -> AuthFlow {
    AuthFlow::LocalCallback {
        authorize_url: GOOGLE_AUTHORIZE_ENDPOINT.to_owned(),
        token_endpoint: GOOGLE_TOKEN_ENDPOINT.to_owned(),
        redirect_path: CALLBACK_REDIRECT_PATH.to_owned(),
        scopes: YOUTUBE_BROADCASTER_SCOPES
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

#[derive(Debug, Clone)]
pub struct LoopbackCode {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub struct YoutubeAuthBundle {
    pub access_token: OAuthToken,
    pub refresh_token: OAuthToken,
    pub channel_id: String,
    pub channel_title: String,
    pub client_id: String,
    pub expires_at: SystemTime,
}

pub fn client_credentials() -> Option<(String, String)> {
    let id = option_env!("FORGE_YOUTUBE_CLIENT_ID")?;
    let secret = option_env!("FORGE_YOUTUBE_CLIENT_SECRET")?;
    if id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((id.to_owned(), secret.to_owned()))
}

pub struct GoogleAuthFlow {
    client: reqwest::Client,
    client_id: String,
    client_secret: String,
    authorize_endpoint: String,
    token_endpoint: String,
    channels_endpoint: String,
    force_consent: bool,
    pending: Option<LocalCallbackDriver>,
}

impl GoogleAuthFlow {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self::with_endpoints(
            client_id,
            client_secret,
            GOOGLE_AUTHORIZE_ENDPOINT.to_owned(),
            GOOGLE_TOKEN_ENDPOINT.to_owned(),
            YOUTUBE_CHANNELS_ENDPOINT.to_owned(),
        )
    }

    pub(crate) fn with_endpoints(
        client_id: String,
        client_secret: String,
        authorize_endpoint: String,
        token_endpoint: String,
        channels_endpoint: String,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id,
            client_secret,
            authorize_endpoint,
            token_endpoint,
            channels_endpoint,
            force_consent: false,
            pending: None,
        }
    }

    /// When `true`, the next `start()` appends `prompt=consent` to the auth URL,
    /// forcing Google's consent screen and guaranteeing a fresh `refresh_token`.
    pub fn set_force_consent(&mut self, force: bool) {
        self.force_consent = force;
    }

    pub(crate) fn client_secret(&self) -> &str {
        &self.client_secret
    }

    pub(crate) fn token_endpoint(&self) -> &str {
        &self.token_endpoint
    }

    pub(crate) fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Binds a loopback listener, generates PKCE + state, stores the driver, and
    /// returns the URL the caller should open in the user's browser.
    pub async fn start(&mut self) -> Result<LoopbackCode, YoutubeAuthError> {
        let driver = LocalCallbackDriver::bind().await?;
        let auth_url = build_authorize_url(
            &self.authorize_endpoint,
            &self.client_id,
            &driver,
            self.force_consent,
        )?;
        self.pending = Some(driver);
        Ok(LoopbackCode { auth_url })
    }

    /// Consumes the pending driver, awaits the loopback callback, exchanges the
    /// authorization code for tokens (PKCE + Google's `client_secret`), and
    /// resolves the broadcaster's YouTube channel.
    pub async fn wait_for_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<YoutubeAuthBundle, YoutubeAuthError> {
        let driver = self.pending.take().ok_or(YoutubeAuthError::NotStarted)?;
        let redirect_uri = driver.redirect_uri().to_owned();
        let code_verifier = driver.code_verifier().to_owned();
        let callback = driver.await_callback(timeout).await?;

        let response = self
            .client
            .post(&self.token_endpoint)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", callback.code.as_str()),
                ("grant_type", "authorization_code"),
                ("redirect_uri", redirect_uri.as_str()),
                ("code_verifier", code_verifier.as_str()),
            ])
            .send()
            .await
            .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(YoutubeAuthError::Http { status, body });
        }
        let token: TokenSuccessResponse = response
            .json()
            .await
            .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

        let refresh_token =
            token
                .refresh_token
                .ok_or_else(|| YoutubeAuthError::ReauthRequired {
                    hint: "Google did not return a refresh_token; retry with consent prompt"
                        .to_owned(),
                })?;

        self.resolve_channel(token.access_token, refresh_token, token.expires_in)
            .await
    }

    async fn resolve_channel(
        &self,
        access_token: String,
        refresh_token: String,
        expires_in: u64,
    ) -> Result<YoutubeAuthBundle, YoutubeAuthError> {
        let response = self
            .client
            .get(&self.channels_endpoint)
            .query(&[("part", "snippet"), ("mine", "true")])
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {access_token}"),
            )
            .send()
            .await
            .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(YoutubeAuthError::Http { status, body });
        }

        let channels: ChannelsListResponse = response
            .json()
            .await
            .map_err(|e| YoutubeAuthError::Network(e.to_string()))?;

        let channel = channels
            .items
            .into_iter()
            .next()
            .ok_or_else(|| YoutubeAuthError::Http {
                status: 200,
                body: "no channel found".to_owned(),
            })?;

        Ok(YoutubeAuthBundle {
            access_token: OAuthToken::new(access_token),
            refresh_token: OAuthToken::new(refresh_token),
            channel_id: channel.id,
            channel_title: channel.snippet.title,
            client_id: self.client_id.clone(),
            expires_at: SystemTime::now() + Duration::from_secs(expires_in),
        })
    }
}

fn build_authorize_url(
    endpoint: &str,
    client_id: &str,
    driver: &LocalCallbackDriver,
    force_consent: bool,
) -> Result<String, YoutubeAuthError> {
    let scope_string = YOUTUBE_BROADCASTER_SCOPES.join(" ");
    let mut params: Vec<(&str, &str)> = vec![
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", driver.redirect_uri()),
        ("scope", scope_string.as_str()),
        ("state", driver.state()),
        ("code_challenge", driver.code_challenge()),
        ("code_challenge_method", "S256"),
        ("access_type", "offline"),
    ];
    if force_consent {
        params.push(("prompt", "consent"));
    }
    let url = reqwest::Url::parse_with_params(endpoint, &params)
        .map_err(|e| YoutubeAuthError::Network(format!("invalid authorize endpoint URL: {e}")))?;
    Ok(url.into())
}

#[derive(Deserialize)]
struct TokenSuccessResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Deserialize)]
struct ChannelsListResponse {
    items: Vec<ChannelItem>,
}

#[derive(Deserialize)]
struct ChannelItem {
    id: String,
    snippet: ChannelSnippet,
}

#[derive(Deserialize)]
struct ChannelSnippet {
    title: String,
}

#[derive(Debug, Error)]
pub enum YoutubeAuthError {
    #[error("HTTP error {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("wait_for_authorization called before start")]
    NotStarted,
    /// Token response was missing `refresh_token`. Caller should retry with
    /// `set_force_consent(true)` to force Google's consent screen.
    #[error("re-authentication required: {hint}")]
    ReauthRequired { hint: String },
    #[error("loopback callback failure: {0}")]
    Loopback(#[from] PlatformError),
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_platform_core::AuthFlow;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn token_success_body() -> serde_json::Value {
        json!({
            "access_token": "ya29.test_access",
            "refresh_token": "1//test_refresh",
            "expires_in": 3599,
            "token_type": "Bearer",
            "scope": "https://www.googleapis.com/auth/youtube.force-ssl"
        })
    }

    fn channel_body() -> serde_json::Value {
        json!({
            "items": [
                {
                    "id": "UCtest123",
                    "snippet": { "title": "Test Channel" }
                }
            ]
        })
    }

    async fn mount_channel_mock(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body()))
            .mount(server)
            .await;
    }

    #[test]
    fn client_credentials_result_is_consistent() {
        let result = client_credentials();
        if let Some((id, secret)) = result {
            assert!(!id.is_empty());
            assert!(!secret.is_empty());
        }
    }

    #[test]
    fn youtube_auth_flow_returns_local_callback_variant() {
        let flow = youtube_auth_flow();
        let AuthFlow::LocalCallback {
            authorize_url,
            token_endpoint,
            redirect_path,
            scopes,
        } = flow
        else {
            unreachable!("youtube_auth_flow must return LocalCallback variant");
        };
        assert_eq!(authorize_url, GOOGLE_AUTHORIZE_ENDPOINT);
        assert_eq!(token_endpoint, GOOGLE_TOKEN_ENDPOINT);
        assert_eq!(redirect_path, CALLBACK_REDIRECT_PATH);
        assert_eq!(
            scopes,
            YOUTUBE_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn scope_list_contains_force_ssl() {
        assert!(
            YOUTUBE_BROADCASTER_SCOPES
                .iter()
                .any(|s| s.contains("youtube.force-ssl"))
        );
    }

    #[tokio::test]
    async fn authorize_url_contains_pkce_state_and_offline_access() {
        let driver = LocalCallbackDriver::bind().await.unwrap();
        let url =
            build_authorize_url(GOOGLE_AUTHORIZE_ENDPOINT, "test_client", &driver, false).unwrap();
        assert!(url.starts_with(GOOGLE_AUTHORIZE_ENDPOINT));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=test_client"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&format!("code_challenge={}", driver.code_challenge())));
        assert!(url.contains(&format!("state={}", driver.state())));
        assert!(url.contains("access_type=offline"));
        assert!(!url.contains("prompt=consent"));
    }

    #[tokio::test]
    async fn authorize_url_appends_prompt_when_force_consent_enabled() {
        let driver = LocalCallbackDriver::bind().await.unwrap();
        let url =
            build_authorize_url(GOOGLE_AUTHORIZE_ENDPOINT, "test_client", &driver, true).unwrap();
        assert!(url.contains("prompt=consent"));
    }

    #[tokio::test]
    async fn wait_for_authorization_returns_not_started_when_no_pending() {
        let mut flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            "http://example.com/auth".to_owned(),
            "http://example.com/token".to_owned(),
            "http://example.com/channels".to_owned(),
        );
        let err = flow
            .wait_for_authorization(Duration::from_millis(10))
            .await
            .unwrap_err();
        assert!(matches!(err, YoutubeAuthError::NotStarted));
    }

    #[tokio::test]
    async fn token_response_without_refresh_token_returns_reauth_required() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "ya29.access_only",
                "expires_in": 3599,
                "token_type": "Bearer",
            })))
            .mount(&server)
            .await;
        mount_channel_mock(&server).await;

        let mut flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/auth", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );
        flow.pending = Some(LocalCallbackDriver::bind().await.unwrap());

        // Send a synthetic callback to the bound port so await_callback resolves.
        let driver = flow.pending.as_ref().unwrap();
        let redirect_uri = driver.redirect_uri().to_owned();
        let state = driver.state().to_owned();
        spawn_callback(&redirect_uri, &state, "auth_code_xyz").await;

        let err = flow
            .wait_for_authorization(Duration::from_secs(2))
            .await
            .unwrap_err();
        assert!(
            matches!(err, YoutubeAuthError::ReauthRequired { ref hint } if hint.contains("refresh_token")),
            "expected ReauthRequired, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn wait_for_authorization_exchanges_code_and_resolves_channel() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code_verifier="))
            .and(body_string_contains("client_secret=csec"))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_success_body()))
            .mount(&server)
            .await;
        mount_channel_mock(&server).await;

        let mut flow = GoogleAuthFlow::with_endpoints(
            "cid".to_owned(),
            "csec".to_owned(),
            format!("{}/auth", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );
        flow.pending = Some(LocalCallbackDriver::bind().await.unwrap());

        let driver = flow.pending.as_ref().unwrap();
        let redirect_uri = driver.redirect_uri().to_owned();
        let state = driver.state().to_owned();
        spawn_callback(&redirect_uri, &state, "auth_code_abc").await;

        let bundle = flow
            .wait_for_authorization(Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(bundle.channel_id, "UCtest123");
        assert_eq!(bundle.channel_title, "Test Channel");
        assert_eq!(bundle.client_id, "cid");
    }

    /// Fires a single HTTP GET against the loopback driver's redirect URI on a
    /// background task - simulates the browser delivering the OAuth callback.
    async fn spawn_callback(redirect_uri: &str, state: &str, code: &str) {
        let url = format!("{redirect_uri}?code={code}&state={state}");
        tokio::spawn(async move {
            // Small delay so the driver's listener is already in accept().
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = reqwest::Client::new().get(&url).send().await;
        });
    }
}
