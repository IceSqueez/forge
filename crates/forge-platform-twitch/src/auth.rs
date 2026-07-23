use std::time::{Duration, SystemTime};

use forge_platform_core::{AuthFlow, PlatformError};
use forge_types::OAuthToken;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use twitch_api::HelixClient;
use twitch_api::twitch_oauth2::{AccessToken, ClientId, TwitchToken, UserToken};

pub const TWITCH_DEVICE_ENDPOINT: &str = "https://id.twitch.tv/oauth2/device";
pub const TWITCH_TOKEN_ENDPOINT: &str = "https://id.twitch.tv/oauth2/token";

const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const SLOW_DOWN_INCREMENT: Duration = Duration::from_secs(5);

pub const TWITCH_BROADCASTER_SCOPES: &[&str] = &[
    "chat:read",
    "chat:edit",
    "user:read:chat",
    "user:write:chat",
    "user:read:whispers",
    "user:manage:whispers",
    "channel:read:subscriptions",
    "bits:read",
    "channel:read:hype_train",
    "channel:read:charity",
    "channel:read:goals",
    "moderator:read:followers",
    "moderation:read",
    "moderator:read:suspicious_users",
    "moderator:read:automod_settings",
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
    "channel:manage:broadcast",
    "channel:manage:moderators",
    "channel:manage:raids",
    "channel:manage:vips",
    "channel:read:redemptions",
    "channel:manage:redemptions",
    "channel:manage:polls",
    "channel:manage:predictions",
    "channel:read:ads",
    "channel:manage:ads",
    "channel:edit:commercial",
    "channel:manage:guest_star",
];

pub fn twitch_auth_flow() -> AuthFlow {
    AuthFlow::DeviceCode {
        user_code_endpoint: TWITCH_DEVICE_ENDPOINT.to_owned(),
        token_endpoint: TWITCH_TOKEN_ENDPOINT.to_owned(),
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
pub struct DeviceCodeInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: SystemTime,
}

#[derive(Clone)]
pub struct TwitchAuthBundle {
    pub access_token: OAuthToken,
    /// Absent routes the first expiry to re-auth instead of a silent renewal.
    pub refresh_token: Option<OAuthToken>,
    pub user_info: UserInfo,
    pub client_id: String,
    /// `None` if the upstream token never expires.
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

enum DevicePollOutcome {
    Pending,
    SlowDown,
    Granted(DeviceTokenResponse),
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct DeviceTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Deserialize)]
struct DevicePollErrorBody {
    message: String,
}

pub struct TwitchAuthFlow {
    client_id: String,
    device_endpoint: String,
    token_endpoint: String,
    http: reqwest::Client,
    helix: HelixClient<'static, reqwest::Client>,
    device_code: Option<String>,
    poll_interval: Duration,
    expires_at: Option<SystemTime>,
}

impl TwitchAuthFlow {
    pub fn new(client_id: String) -> Self {
        Self::with_endpoints(
            client_id,
            TWITCH_DEVICE_ENDPOINT.to_owned(),
            TWITCH_TOKEN_ENDPOINT.to_owned(),
        )
    }

    pub(crate) fn with_endpoints(
        client_id: String,
        device_endpoint: String,
        token_endpoint: String,
    ) -> Self {
        Self {
            client_id,
            device_endpoint,
            token_endpoint,
            http: reqwest::Client::new(),
            helix: HelixClient::with_client(reqwest::Client::new()),
            device_code: None,
            poll_interval: Duration::from_secs(5),
            expires_at: None,
        }
    }

    pub async fn start(&mut self) -> Result<DeviceCodeInfo, PlatformError> {
        let scopes = TWITCH_BROADCASTER_SCOPES.join(" ");
        let form = [
            ("client_id", self.client_id.as_str()),
            ("scopes", scopes.as_str()),
        ];
        let (status, body) = post_form(&self.http, &self.device_endpoint, &form).await?;
        if status != 200 {
            return Err(PlatformError::Http { status, body });
        }
        let parsed: DeviceCodeResponse = serde_json::from_str(&body)?;
        let expires_at = SystemTime::now() + Duration::from_secs(parsed.expires_in);
        self.device_code = Some(parsed.device_code);
        self.poll_interval = Duration::from_secs(parsed.interval.max(1));
        self.expires_at = Some(expires_at);
        Ok(DeviceCodeInfo {
            user_code: parsed.user_code,
            verification_uri: parsed.verification_uri,
            expires_at,
        })
    }

    /// Stops as soon as `cancel` fires; otherwise polls until Twitch grants a token,
    /// rejects the code, or the device code's own `expires_in` deadline passes.
    pub async fn wait_for_authorization(
        &mut self,
        cancel: CancellationToken,
    ) -> Result<TwitchAuthBundle, PlatformError> {
        let device_code = self
            .device_code
            .clone()
            .ok_or_else(|| PlatformError::Auth {
                reason: "wait_for_authorization called before start".into(),
            })?;
        let expires_at = self.expires_at.ok_or_else(|| PlatformError::Auth {
            reason: "wait_for_authorization called before start".into(),
        })?;

        let token_response = loop {
            if SystemTime::now() >= expires_at {
                return Err(PlatformError::Auth {
                    reason: "device code expired".into(),
                });
            }
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Err(PlatformError::Auth {
                        reason: "device authorization cancelled".into(),
                    });
                }
                _ = tokio::time::sleep(self.poll_interval) => {}
            }
            match self.poll_token(&device_code).await? {
                DevicePollOutcome::Pending => continue,
                DevicePollOutcome::SlowDown => {
                    self.poll_interval += SLOW_DOWN_INCREMENT;
                    continue;
                }
                DevicePollOutcome::Granted(response) => break response,
            }
        };

        self.finish(token_response).await
    }

    async fn poll_token(&self, device_code: &str) -> Result<DevicePollOutcome, PlatformError> {
        let form = [
            ("client_id", self.client_id.as_str()),
            ("device_code", device_code),
            ("grant_type", DEVICE_GRANT_TYPE),
        ];
        let (status, body) = post_form(&self.http, &self.token_endpoint, &form).await?;
        if status == 200 {
            let parsed: DeviceTokenResponse = serde_json::from_str(&body)?;
            return Ok(DevicePollOutcome::Granted(parsed));
        }

        let message = serde_json::from_str::<DevicePollErrorBody>(&body)
            .map(|e| e.message)
            .unwrap_or_default();
        match message.as_str() {
            "authorization_pending" => Ok(DevicePollOutcome::Pending),
            "slow_down" => Ok(DevicePollOutcome::SlowDown),
            "expired_token" | "invalid device code" => Err(PlatformError::Auth {
                reason: "device code expired or already used".into(),
            }),
            "access_denied" => Err(PlatformError::Auth {
                reason: "authorization denied by user".into(),
            }),
            _ => Err(PlatformError::Http {
                status,
                body: message,
            }),
        }
    }

    async fn finish(
        &self,
        token_response: DeviceTokenResponse,
    ) -> Result<TwitchAuthBundle, PlatformError> {
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
            .map(|secs| SystemTime::now() + Duration::from_secs(secs))
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

async fn post_form(
    http: &reqwest::Client,
    url: &str,
    form: &[(&str, &str)],
) -> Result<(u16, String), PlatformError> {
    let resp = tokio::time::timeout(REQUEST_TIMEOUT, http.post(url).form(form).send())
        .await
        .map_err(|_| PlatformError::Network {
            reason: "request timed out".into(),
        })?
        .map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;
    let status = resp.status().as_u16();
    let body = tokio::time::timeout(REQUEST_TIMEOUT, resp.text())
        .await
        .map_err(|_| PlatformError::Network {
            reason: "response read timed out".into(),
        })?
        .map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;
    Ok((status, body))
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

/// Priority: runtime env `FORGE_TWITCH_CLIENT_ID` -> compile-time `option_env!` -> `None`.
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
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn flow_for(device_url: String, token_url: String) -> TwitchAuthFlow {
        TwitchAuthFlow::with_endpoints("test_client".to_owned(), device_url, token_url)
    }

    async fn mount_device(server: &MockServer, expires_in: u64, interval: u64) {
        Mock::given(method("POST"))
            .and(path("/device"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "DEV123",
                "user_code": "WXYZ-1234",
                "verification_uri": "https://www.twitch.tv/activate",
                "expires_in": expires_in,
                "interval": interval,
            })))
            .mount(server)
            .await;
    }

    async fn mount_token_message(server: &MockServer, status: u16, message: &str) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(serde_json::json!({ "message": message })),
            )
            .mount(server)
            .await;
    }

    async fn first_body(server: &MockServer) -> String {
        let reqs = server.received_requests().await.unwrap();
        String::from_utf8_lossy(&reqs[0].body).into_owned()
    }

    #[tokio::test]
    async fn start_requests_scopes_and_parses_device_code_response() {
        let server = MockServer::start().await;
        mount_device(&server, 1800, 5).await;
        let mut flow = flow_for(
            format!("{}/device", server.uri()),
            "http://unused".to_owned(),
        );

        let info = flow.start().await.unwrap();

        assert_eq!(info.user_code, "WXYZ-1234");
        assert_eq!(info.verification_uri, "https://www.twitch.tv/activate");
        let remaining = info.expires_at.duration_since(SystemTime::now()).unwrap();
        assert!(
            remaining > Duration::from_secs(1700) && remaining <= Duration::from_secs(1800),
            "expires_at must reflect expires_in, got {remaining:?}",
        );

        let body = first_body(&server).await;
        assert!(body.contains("client_id=test_client"), "{body}");
        assert!(body.contains("scopes="), "{body}");
    }

    #[tokio::test]
    async fn start_maps_non_200_device_response_to_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/device"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad client"))
            .mount(&server)
            .await;
        let mut flow = flow_for(
            format!("{}/device", server.uri()),
            "http://unused".to_owned(),
        );

        let err = flow.start().await.unwrap_err();

        assert!(matches!(err, PlatformError::Http { status: 400, .. }));
    }

    #[tokio::test]
    async fn poll_token_grant_carries_device_code_urn_grant_and_omits_scopes() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "ACCESS_XYZ",
                "refresh_token": "REFRESH_XYZ",
                "expires_in": 3600,
            })))
            .mount(&server)
            .await;
        let flow = flow_for(
            "http://unused".to_owned(),
            format!("{}/token", server.uri()),
        );

        let outcome = flow.poll_token("DEV123").await.unwrap();

        let DevicePollOutcome::Granted(resp) = outcome else {
            panic!("expected Granted outcome");
        };
        assert_eq!(resp.access_token, "ACCESS_XYZ");
        assert_eq!(resp.refresh_token.as_deref(), Some("REFRESH_XYZ"));

        let body = first_body(&server).await;
        assert!(body.contains("device_code=DEV123"), "{body}");
        assert!(body.contains("grant_type=urn"), "{body}");
        assert!(
            !body.contains("scopes"),
            "poll must not send scopes: {body}"
        );
    }

    #[tokio::test]
    async fn poll_token_maps_pending_and_slow_down_to_retry_outcomes() {
        for (message, want_slow_down) in [("authorization_pending", false), ("slow_down", true)] {
            let server = MockServer::start().await;
            mount_token_message(&server, 400, message).await;
            let flow = flow_for(
                "http://unused".to_owned(),
                format!("{}/token", server.uri()),
            );

            let outcome = flow.poll_token("DEV123").await.unwrap();

            match (outcome, want_slow_down) {
                (DevicePollOutcome::Pending, false) => {}
                (DevicePollOutcome::SlowDown, true) => {}
                _ => panic!("wrong retry outcome for {message}"),
            }
        }
    }

    #[tokio::test]
    async fn poll_token_maps_terminal_errors_to_distinct_auth_reasons() {
        for (message, must_contain, must_not_contain) in [
            ("access_denied", "denied", "expired"),
            ("expired_token", "expired", "denied"),
        ] {
            let server = MockServer::start().await;
            mount_token_message(&server, 400, message).await;
            let flow = flow_for(
                "http://unused".to_owned(),
                format!("{}/token", server.uri()),
            );

            let Err(PlatformError::Auth { reason }) = flow.poll_token("DEV123").await else {
                panic!("expected Auth error for {message}");
            };
            assert!(reason.contains(must_contain), "{message}: {reason}");
            assert!(!reason.contains(must_not_contain), "{message}: {reason}");
        }
    }

    #[tokio::test]
    async fn poll_token_maps_unknown_error_message_to_http_error() {
        let server = MockServer::start().await;
        mount_token_message(&server, 418, "teapot").await;
        let flow = flow_for(
            "http://unused".to_owned(),
            format!("{}/token", server.uri()),
        );

        let outcome = flow.poll_token("DEV123").await;

        assert!(matches!(
            outcome,
            Err(PlatformError::Http { status: 418, .. })
        ));
    }

    #[tokio::test]
    async fn wait_before_start_reports_auth_error() {
        let mut flow = flow_for("http://unused".to_owned(), "http://unused".to_owned());

        let err = flow
            .wait_for_authorization(CancellationToken::new())
            .await
            .unwrap_err();

        assert!(matches!(err, PlatformError::Auth { .. }));
    }

    #[tokio::test]
    async fn wait_cancelled_before_first_poll_returns_promptly_without_polling() {
        let device_server = MockServer::start().await;
        mount_device(&device_server, 3600, 5).await;
        let token_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&token_server)
            .await;
        let mut flow = flow_for(
            format!("{}/device", device_server.uri()),
            format!("{}/token", token_server.uri()),
        );
        flow.start().await.unwrap();

        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = flow.wait_for_authorization(cancel).await.unwrap_err();

        let PlatformError::Auth { reason } = err else {
            panic!("expected Auth error");
        };
        // Why: forge-desktop classifies this Display by the substring "cancelled";
        // the reason wording is a cross-crate contract consumed by twitch_panel.
        assert!(reason.contains("cancelled"), "{reason}");
        assert!(
            token_server.received_requests().await.unwrap().is_empty(),
            "cancellation must abort before any token poll",
        );
    }

    #[tokio::test]
    async fn wait_polls_again_after_authorization_pending_until_terminal() {
        let token_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "message": "authorization_pending" })),
            )
            .up_to_n_times(1)
            .mount(&token_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "message": "access_denied" })),
            )
            .mount(&token_server)
            .await;
        let mut flow = flow_for(
            "http://unused".to_owned(),
            format!("{}/token", token_server.uri()),
        );
        flow.device_code = Some("DEV123".to_owned());
        flow.expires_at = Some(SystemTime::now() + Duration::from_secs(3600));
        flow.poll_interval = Duration::from_millis(5);

        let err = flow
            .wait_for_authorization(CancellationToken::new())
            .await
            .unwrap_err();

        assert!(matches!(err, PlatformError::Auth { .. }));
        assert_eq!(
            token_server.received_requests().await.unwrap().len(),
            2,
            "loop must poll again after authorization_pending",
        );
    }

    #[test]
    fn resolve_client_id_prefers_runtime_then_compile_ignoring_empties() {
        for (runtime, compile, expected) in [
            (Some("runtime_id"), Some("compile_id"), Some("runtime_id")),
            (None, Some("compile_id"), Some("compile_id")),
            (Some(""), Some("compile_id"), Some("compile_id")),
            (None, Some(""), None),
            (None, None, None),
        ] {
            assert_eq!(
                resolve_client_id(runtime, compile),
                expected.map(str::to_owned),
                "runtime={runtime:?} compile={compile:?}",
            );
        }
    }

    #[test]
    fn twitch_auth_flow_is_device_code_grant() {
        // Why: Twitch deliberately uses the Device Authorization Grant; the loopback/PKCE
        // flow was removed. Pin that product decision against accidental reversion.
        let AuthFlow::DeviceCode { scopes, .. } = twitch_auth_flow() else {
            panic!("twitch must use the device-code auth flow");
        };
        assert!(!scopes.is_empty(), "device grant must request scopes");
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
}
