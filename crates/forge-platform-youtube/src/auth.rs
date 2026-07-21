use std::time::{Duration, SystemTime};

use forge_platform_core::auth::{
    CALLBACK_PATH, PkceClientConfig, PkceFlow, PkceRefreshConfig, ReauthPolicy,
};
use forge_platform_core::{AuthFlow, PlatformError};
use forge_types::OAuthToken;
use serde::Deserialize;

pub const GOOGLE_AUTHORIZE_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

const YOUTUBE_CHANNELS_ENDPOINT: &str = "https://www.googleapis.com/youtube/v3/channels";

pub const YOUTUBE_BROADCASTER_SCOPES: &[&str] =
    &["https://www.googleapis.com/auth/youtube.force-ssl"];

const PLATFORM: &str = "youtube";

pub fn youtube_auth_flow() -> AuthFlow {
    AuthFlow::LocalCallback {
        authorize_url: GOOGLE_AUTHORIZE_ENDPOINT.to_owned(),
        token_endpoint: GOOGLE_TOKEN_ENDPOINT.to_owned(),
        redirect_path: CALLBACK_PATH.to_owned(),
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
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    token_endpoint: String,
    channels_endpoint: String,
    force_consent: bool,
    pkce: PkceFlow,
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
        let pkce = PkceFlow::new(PkceClientConfig {
            client_id: client_id.clone(),
            client_secret: Some(client_secret.clone()),
            authorize_endpoint,
            token_endpoint: token_endpoint.clone(),
            scopes: YOUTUBE_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            authorize_pre_redirect_params: Vec::new(),
            authorize_trailing_params: vec![("access_type".to_owned(), "offline".to_owned())],
        });
        Self {
            http: reqwest::Client::new(),
            client_id,
            client_secret,
            token_endpoint,
            channels_endpoint,
            force_consent: false,
            pkce,
        }
    }

    /// When `true`, the next `start()` appends `prompt=consent` to the auth URL,
    /// forcing Google's consent screen and guaranteeing a fresh `refresh_token`.
    pub fn set_force_consent(&mut self, force: bool) {
        self.force_consent = force;
    }

    pub(crate) fn refresh_config(&self) -> PkceRefreshConfig {
        PkceRefreshConfig {
            platform: PLATFORM.to_owned(),
            client_id: self.client_id.clone(),
            client_secret: Some(self.client_secret.clone()),
            token_endpoint: self.token_endpoint.clone(),
            reauth_policy: ReauthPolicy::InvalidGrantOn400,
        }
    }

    /// Binds a loopback listener, generates PKCE + state, stores the driver, and
    /// returns the URL the caller should open in the user's browser.
    pub async fn start(&mut self) -> Result<LoopbackCode, PlatformError> {
        let extra: &[(&str, &str)] = if self.force_consent {
            &[("prompt", "consent")]
        } else {
            &[]
        };
        let url = self.pkce.start(extra).await?;
        Ok(LoopbackCode {
            auth_url: url.auth_url,
        })
    }

    /// Consumes the pending driver, awaits the loopback callback, exchanges the
    /// authorization code for tokens (PKCE + Google's `client_secret`), and
    /// resolves the broadcaster's YouTube channel.
    pub async fn wait_for_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<YoutubeAuthBundle, PlatformError> {
        let token = self.pkce.exchange(timeout).await?;
        let refresh_token = token.refresh_token.ok_or_else(|| PlatformError::Auth {
            reason: "Google did not return a refresh_token; retry with consent prompt".into(),
        })?;

        self.resolve_channel(
            token.access_token,
            refresh_token,
            token.expires_in.unwrap_or(0),
        )
        .await
    }

    async fn resolve_channel(
        &self,
        access_token: String,
        refresh_token: String,
        expires_in: u64,
    ) -> Result<YoutubeAuthBundle, PlatformError> {
        let response = self
            .http
            .get(&self.channels_endpoint)
            .query(&[("part", "snippet"), ("mine", "true")])
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {access_token}"),
            )
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(PlatformError::Http { status, body });
        }

        let channels: ChannelsListResponse =
            response.json().await.map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let channel = channels
            .items
            .into_iter()
            .next()
            .ok_or_else(|| PlatformError::Http {
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
        assert_eq!(redirect_path, CALLBACK_PATH);
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
    async fn start_builds_authorize_url_with_offline_access_and_no_prompt_by_default() {
        let mut flow = GoogleAuthFlow::new("test_client".to_owned(), "test_secret".to_owned());
        let url = flow.start().await.unwrap().auth_url;
        assert!(url.starts_with(GOOGLE_AUTHORIZE_ENDPOINT));
        assert!(url.contains("client_id=test_client"));
        assert!(url.contains("access_type=offline"));
        assert!(!url.contains("prompt=consent"));
    }

    #[tokio::test]
    async fn start_appends_prompt_when_force_consent_enabled() {
        let mut flow = GoogleAuthFlow::new("test_client".to_owned(), "test_secret".to_owned());
        flow.set_force_consent(true);
        let url = flow.start().await.unwrap().auth_url;
        assert!(url.contains("prompt=consent"));
    }

    #[tokio::test]
    async fn token_response_without_refresh_token_returns_auth_error() {
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
        let auth_url = flow.start().await.unwrap().auth_url;
        trigger_callback(&auth_url, "auth_code_xyz").await;

        let err = flow
            .wait_for_authorization(Duration::from_secs(2))
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::Auth { ref reason } if reason.contains("refresh_token")),
            "expected Auth error mentioning refresh_token, got: {err:?}"
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
        let auth_url = flow.start().await.unwrap().auth_url;
        trigger_callback(&auth_url, "auth_code_abc").await;

        let bundle = flow
            .wait_for_authorization(Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(bundle.channel_id, "UCtest123");
        assert_eq!(bundle.channel_title, "Test Channel");
        assert_eq!(bundle.client_id, "cid");
    }

    async fn trigger_callback(auth_url: &str, code: &str) {
        let redirect_uri = auth_url
            .split("redirect_uri=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .replace("%3A", ":")
            .replace("%2F", "/");
        let state = auth_url
            .split("state=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        let url = format!("{redirect_uri}?code={code}&state={state}");
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = reqwest::Client::new().get(&url).send().await;
        });
    }
}
