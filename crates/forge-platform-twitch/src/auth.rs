use std::time::Duration;

use forge_platform_core::auth::LocalCallbackDriver;
use forge_platform_core::{AuthFlow, PlatformError};
use forge_types::OAuthToken;
use serde::Deserialize;
use twitch_api::HelixClient;
use twitch_api::twitch_oauth2::{AccessToken, ClientId, TwitchToken, UserToken};

pub const TWITCH_AUTHORIZE_ENDPOINT: &str = "https://id.twitch.tv/oauth2/authorize";
pub const TWITCH_TOKEN_ENDPOINT: &str = "https://id.twitch.tv/oauth2/token";

pub const TWITCH_BROADCASTER_SCOPES: &[&str] = &[
    // Chat I/O
    "chat:read",
    "chat:edit",
    "user:read:chat",
    "user:write:chat",
    "user:read:whispers",
    "user:manage:whispers",
    // Support / loyalty events
    "channel:read:subscriptions",
    "bits:read",
    "channel:read:hype_train",
    "channel:read:charity",
    "channel:read:goals",
    // Follow events
    "moderator:read:followers",
    // Moderation - read
    "moderation:read",
    "moderator:read:suspicious_users",
    "moderator:read:automod_settings",
    // Moderation - manage
    "channel:moderate",
    "moderator:manage:announcements",
    "moderator:manage:automod",
    "moderator:manage:automod_settings",
    "moderator:manage:banned_users",
    "moderator:manage:blocked_terms",
    "moderator:manage:chat_messages",
    "moderator:manage:chat_settings",
    "moderator:manage:shield_mode",
    "moderator:manage:shoutouts",
    "moderator:manage:unban_requests",
    "moderator:manage:warnings",
    // Channel management
    "channel:manage:broadcast",
    "user:manage:broadcast",
    "channel:manage:moderators",
    "channel:manage:raids",
    "channel:manage:vips",
    // Channel Points
    "channel:read:redemptions",
    "channel:manage:redemptions",
    // Polls / Predictions
    "channel:manage:polls",
    "channel:manage:predictions",
    // Ads
    "channel:read:ads",
    "channel:manage:ads",
    "channel:edit:commercial",
    // Guest Star (beta)
    "channel:manage:guest_star",
];

const CALLBACK_REDIRECT_PATH: &str = "/oauth/callback";

pub fn twitch_auth_flow() -> AuthFlow {
    AuthFlow::LocalCallback {
        authorize_url: TWITCH_AUTHORIZE_ENDPOINT.to_owned(),
        token_endpoint: TWITCH_TOKEN_ENDPOINT.to_owned(),
        redirect_path: CALLBACK_REDIRECT_PATH.to_owned(),
        scopes: TWITCH_BROADCASTER_SCOPES
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: String,
    pub login: String,
    pub display_name: String,
}

#[derive(Debug, Clone)]
pub struct LoopbackCode {
    pub auth_url: String,
}

#[derive(Clone)]
pub struct TwitchAuthBundle {
    pub access_token: OAuthToken,
    /// Absent when the grant carried no refresh token; the credential then
    /// routes its first expiry to re-auth instead of a silent renewal.
    pub refresh_token: Option<OAuthToken>,
    pub user_info: UserInfo,
    pub client_id: String,
    /// Absolute expiry time. `None` if the upstream token never expires.
    pub expires_at: Option<std::time::SystemTime>,
}

impl std::fmt::Debug for TwitchAuthBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TwitchAuthBundle")
            .field("access_token", &self.access_token)
            .field("refresh_token", &self.refresh_token)
            .field("user_info", &self.user_info)
            .field("client_id", &self.client_id)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

pub struct TwitchAuthFlow {
    client_id: String,
    http: reqwest::Client,
    helix: HelixClient<'static, reqwest::Client>,
    authorize_endpoint: String,
    token_endpoint: String,
    pending: Option<LocalCallbackDriver>,
}

impl TwitchAuthFlow {
    pub fn new(client_id: String) -> Self {
        Self::with_endpoints(
            client_id,
            TWITCH_AUTHORIZE_ENDPOINT.to_owned(),
            TWITCH_TOKEN_ENDPOINT.to_owned(),
        )
    }

    pub(crate) fn with_endpoints(
        client_id: String,
        authorize_endpoint: String,
        token_endpoint: String,
    ) -> Self {
        let http = reqwest::Client::new();
        let helix = HelixClient::with_client(http.clone());
        Self {
            client_id,
            http,
            helix,
            authorize_endpoint,
            token_endpoint,
            pending: None,
        }
    }

    /// Binds a loopback listener, generates PKCE + state, stores the driver, and
    /// returns the URL the caller should open in the user's browser. Subsequent
    /// `wait_for_authorization` will consume the stored driver.
    pub async fn start(&mut self) -> Result<LoopbackCode, PlatformError> {
        let driver = LocalCallbackDriver::bind().await?;
        let auth_url = build_authorize_url(&self.authorize_endpoint, &self.client_id, &driver)?;
        self.pending = Some(driver);
        Ok(LoopbackCode { auth_url })
    }

    /// Consumes the pending driver, waits for the loopback callback, exchanges
    /// the code for a Twitch access token (PKCE - no `client_secret`), validates
    /// against Helix, and resolves the broadcaster's profile.
    pub async fn wait_for_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<TwitchAuthBundle, PlatformError> {
        let driver = self.pending.take().ok_or_else(|| PlatformError::Auth {
            reason: "wait_for_authorization called before start".into(),
        })?;
        let redirect_uri = driver.redirect_uri().to_owned();
        let code_verifier = driver.code_verifier().to_owned();
        let callback = driver.await_callback(timeout).await?;

        let token_response = exchange_code(
            &self.http,
            &self.token_endpoint,
            &self.client_id,
            &callback.code,
            &redirect_uri,
            &code_verifier,
        )
        .await?;

        let access = AccessToken::new(token_response.access_token.clone());
        let user_token = UserToken::from_existing(&self.helix, access, None, None)
            .await
            .map_err(|e| PlatformError::Auth {
                reason: format!(
                    "token validate failed: {}",
                    sanitize_validation_error(&e.error)
                ),
            })?;

        if user_token.client_id() != &ClientId::new(self.client_id.clone()) {
            return Err(PlatformError::Auth {
                reason: "token client_id does not match configured client_id".into(),
            });
        }

        let user_info = fetch_user_info_from_token(&user_token, &self.helix).await?;
        let expires_at = token_response
            .expires_in
            .filter(|secs| *secs > 0)
            .map(|secs| std::time::SystemTime::now() + std::time::Duration::from_secs(secs))
            .or_else(|| expires_at_from_token(&user_token));

        Ok(TwitchAuthBundle {
            access_token: OAuthToken::new(token_response.access_token),
            refresh_token: token_response.refresh_token.map(OAuthToken::new),
            user_info,
            client_id: self.client_id.clone(),
            expires_at,
        })
    }
}

fn build_authorize_url(
    endpoint: &str,
    client_id: &str,
    driver: &LocalCallbackDriver,
) -> Result<String, PlatformError> {
    let scope_string = TWITCH_BROADCASTER_SCOPES.join(" ");
    let url = reqwest::Url::parse_with_params(
        endpoint,
        &[
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", driver.redirect_uri()),
            ("scope", scope_string.as_str()),
            ("state", driver.state()),
            ("code_challenge", driver.code_challenge()),
            ("code_challenge_method", "S256"),
        ],
    )
    .map_err(|e| PlatformError::Auth {
        reason: format!("invalid authorize endpoint URL: {e}"),
    })?;
    Ok(url.into())
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

async fn exchange_code(
    http: &reqwest::Client,
    token_endpoint: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<TokenResponse, PlatformError> {
    let resp = http
        .post(token_endpoint)
        .form(&[
            ("client_id", client_id),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await
        .map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;

    let status = resp.status().as_u16();
    if status != 200 {
        let body = resp.text().await.unwrap_or_default();
        return Err(PlatformError::Http { status, body });
    }
    resp.json::<TokenResponse>()
        .await
        .map_err(|e| PlatformError::Network {
            reason: format!("token response parse failed: {e}"),
        })
}

fn expires_at_from_token(token: &UserToken) -> Option<std::time::SystemTime> {
    if token.never_expires() {
        return None;
    }
    Some(std::time::SystemTime::now() + token.expires_in())
}

async fn fetch_user_info_from_token(
    token: &UserToken,
    helix: &HelixClient<'static, reqwest::Client>,
) -> Result<UserInfo, PlatformError> {
    let user = helix
        .get_user_from_id(&token.user_id, token)
        .await
        .map_err(|e| PlatformError::Auth {
            reason: format!("helix get_user failed: {}", sanitize_helix_error(&e)),
        })?
        .ok_or_else(|| PlatformError::Auth {
            reason: "helix returned empty user list".into(),
        })?;
    Ok(UserInfo {
        id: user.id.to_string(),
        login: user.login.to_string(),
        display_name: user.display_name.to_string(),
    })
}

pub async fn fetch_user_info(
    token: &OAuthToken,
    client_id: &str,
) -> Result<UserInfo, PlatformError> {
    let access = AccessToken::new(token.expose().to_owned());
    let client_id_owned = ClientId::new(client_id.to_owned());
    let http = reqwest::Client::new();
    let helix = HelixClient::with_client(http);

    let user_token =
        UserToken::from_token(&helix, access)
            .await
            .map_err(|e| PlatformError::Auth {
                reason: format!(
                    "validate token failed: {}",
                    sanitize_validation_error(&e.error)
                ),
            })?;

    if user_token.client_id() != &client_id_owned {
        return Err(PlatformError::Auth {
            reason: "stored token client_id does not match configured client_id".into(),
        });
    }

    fetch_user_info_from_token(&user_token, &helix).await
}

/// Priority: runtime env `FORGE_TWITCH_CLIENT_ID` → compile-time `option_env!` → `None`.
pub fn client_id() -> Option<String> {
    let runtime = std::env::var("FORGE_TWITCH_CLIENT_ID").ok();
    resolve_client_id(runtime.as_deref(), option_env!("FORGE_TWITCH_CLIENT_ID"))
}

fn resolve_client_id(
    runtime_env: Option<&str>,
    compile_env: Option<&'static str>,
) -> Option<String> {
    runtime_env
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| compile_env.filter(|s| !s.is_empty()).map(str::to_owned))
}

fn sanitize_validation_error<E>(
    e: &twitch_api::twitch_oauth2::tokens::errors::ValidationError<E>,
) -> String
where
    E: std::error::Error + Send + Sync + 'static,
{
    use twitch_api::twitch_oauth2::tokens::errors::ValidationError as VE;
    match e {
        VE::NotAuthorized => "not authorized".to_owned(),
        VE::RequestParseError(_) => "response parse error".to_owned(),
        VE::Request(_) => "network error".to_owned(),
        VE::InvalidToken(s) => format!("invalid token type: {s}"),
        _ => "validation error".to_owned(),
    }
}

fn sanitize_helix_error<E>(e: &twitch_api::helix::ClientRequestError<E>) -> String
where
    E: std::error::Error + Send + Sync + 'static,
{
    use twitch_api::helix::{ClientRequestError as CE, HelixRequestGetError as HRGE};
    match e {
        CE::RequestError(_) => "network error".to_owned(),
        CE::NoPage => "no pagination".to_owned(),
        CE::HelixRequestGetError(HRGE::Error { status, .. }) => {
            format!("HTTP {}", status.as_u16())
        }
        _ => "helix error".to_owned(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn twitch_auth_flow_yields_local_callback_variant() {
        let flow = twitch_auth_flow();
        let AuthFlow::LocalCallback {
            authorize_url,
            token_endpoint,
            redirect_path,
            scopes,
        } = flow
        else {
            unreachable!("twitch_auth_flow must return LocalCallback variant");
        };
        assert_eq!(authorize_url, TWITCH_AUTHORIZE_ENDPOINT);
        assert_eq!(token_endpoint, TWITCH_TOKEN_ENDPOINT);
        assert_eq!(redirect_path, CALLBACK_REDIRECT_PATH);
        assert_eq!(
            scopes,
            TWITCH_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn client_id_prefers_runtime_over_compile_time() {
        assert_eq!(
            resolve_client_id(Some("runtime_id"), Some("compile_id")),
            Some("runtime_id".to_owned()),
        );
    }

    #[test]
    fn client_id_falls_back_to_compile_time_when_runtime_absent() {
        assert_eq!(
            resolve_client_id(None, Some("compile_id")),
            Some("compile_id".to_owned()),
        );
    }

    #[test]
    fn client_id_returns_none_when_both_absent() {
        assert_eq!(resolve_client_id(None, None), None);
    }

    #[test]
    fn client_id_treats_empty_runtime_as_absent() {
        assert_eq!(
            resolve_client_id(Some(""), Some("compile_id")),
            Some("compile_id".to_owned()),
        );
    }

    #[test]
    fn client_id_treats_empty_compile_time_as_absent() {
        assert_eq!(resolve_client_id(None, Some("")), None);
    }

    #[tokio::test]
    async fn authorize_url_contains_required_pkce_params() {
        let driver = LocalCallbackDriver::bind().await.unwrap();
        let url = build_authorize_url(TWITCH_AUTHORIZE_ENDPOINT, "test_client", &driver).unwrap();
        assert!(url.starts_with(TWITCH_AUTHORIZE_ENDPOINT));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=test_client"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains(&format!("code_challenge={}", driver.code_challenge())));
        assert!(url.contains(&format!("state={}", driver.state())));
        assert!(url.contains("scope="));
        assert!(!url.to_lowercase().contains("client_secret"));
    }

    #[tokio::test]
    async fn exchange_code_sends_pkce_form_and_parses_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code_verifier=verifier123"))
            .and(body_string_contains("code=auth_code_xyz"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "twitch_access_abc",
                "token_type": "bearer",
                "scope": ["chat:read"],
                "expires_in": 14400,
            })))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let token_url = format!("{}/token", server.uri());
        let resp = exchange_code(
            &http,
            &token_url,
            "test_client",
            "auth_code_xyz",
            "http://127.0.0.1:0/oauth/callback",
            "verifier123",
        )
        .await
        .unwrap();
        assert_eq!(resp.access_token, "twitch_access_abc");
    }

    /// RFC-091: token exchange now captures refresh_token and expires_in.
    /// Verify both survive the parse and are correctly typed (Option).
    #[tokio::test]
    async fn exchange_code_captures_refresh_token_and_lifetime() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "twitch_access_xyz",
                "refresh_token": "twitch_refresh_xyz",
                "expires_in": 14400,
                "token_type": "bearer",
                "scope": ["chat:read"],
            })))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let resp = exchange_code(
            &http,
            &format!("{}/token", server.uri()),
            "test_client",
            "code_abc",
            "http://127.0.0.1:0/oauth/callback",
            "verifier_abc",
        )
        .await
        .unwrap();

        assert_eq!(resp.access_token, "twitch_access_xyz");
        assert_eq!(resp.refresh_token.as_deref(), Some("twitch_refresh_xyz"));
        assert_eq!(resp.expires_in, Some(14400));
    }

    /// RFC-091: when the upstream response omits refresh_token and expires_in,
    /// both fields deserialise to None (not a parse error).
    #[tokio::test]
    async fn exchange_code_handles_response_without_refresh_token_or_lifetime() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "access_only",
                "token_type": "bearer",
            })))
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let resp = exchange_code(
            &http,
            &format!("{}/token", server.uri()),
            "client",
            "code",
            "http://127.0.0.1:0/callback",
            "verifier",
        )
        .await
        .unwrap();

        assert_eq!(resp.access_token, "access_only");
        assert!(resp.refresh_token.is_none());
        assert!(resp.expires_in.is_none());
    }

    #[test]
    fn helix_error_sanitizer_strips_bearer_from_custom_variant() {
        use std::borrow::Cow;
        use twitch_api::helix::ClientRequestError;

        let e: ClientRequestError<reqwest::Error> =
            ClientRequestError::Custom(Cow::Borrowed("Bearer FAKE_BEARER_VALUE_123 is invalid"));
        let msg = sanitize_helix_error(&e);
        assert!(!msg.contains("FAKE_BEARER_VALUE_123"));
    }

    #[test]
    fn validation_error_sanitizer_excludes_arbitrary_content() {
        use twitch_api::twitch_oauth2::tokens::errors::ValidationError;

        let e: ValidationError<reqwest::Error> = ValidationError::NotAuthorized;
        let msg = sanitize_validation_error(&e);
        assert!(!msg.contains("FAKE_BEARER_VALUE_123"));
        assert!(!msg.is_empty());
    }

    #[tokio::test]
    async fn exchange_code_network_error_strips_url() {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(1))
            .build()
            .unwrap();
        let err = exchange_code(
            &http,
            "https://192.0.2.1/token",
            "client",
            "code",
            "http://127.0.0.1:0/callback",
            "verifier",
        )
        .await
        .unwrap_err();
        assert!(!format!("{err}").contains("192.0.2.1"));
    }

    #[tokio::test]
    async fn exchange_code_propagates_http_error_on_4xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"})),
            )
            .mount(&server)
            .await;

        let http = reqwest::Client::new();
        let token_url = format!("{}/token", server.uri());
        let err = exchange_code(
            &http,
            &token_url,
            "test_client",
            "bad_code",
            "http://127.0.0.1:0/oauth/callback",
            "verifier",
        )
        .await
        .unwrap_err();
        assert!(matches!(err, PlatformError::Http { status: 400, .. }));
    }
}
