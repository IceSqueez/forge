use std::time::{Duration, SystemTime};

use forge_net::{Gateway, GatewayRequest, Secret};
use forge_platform_core::{AuthFlow, EndpointSurface, PlatformEndpoints, PlatformError};
use forge_types::OAuthToken;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace};

use crate::helix::CLIENT_ID_HEADER;
use crate::oauth_client::{
    DevicePoll, TokenGrant, TwitchOAuthClient, ValidatedToken, decode, oauth_error,
};
use crate::sub_actions::identity::BroadcasterTier;

const TWITCH_DEVICE_PATH: &str = "/device";
const TWITCH_TOKEN_PATH: &str = "/token";
const TWITCH_VALIDATE_PATH: &str = "/validate";
const TWITCH_REVOKE_PATH: &str = "/revoke";
const HELIX_USERS_PATH: &str = "/helix/users";

const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const MIN_POLL_INTERVAL_SECS: u64 = 1;
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

pub const TWITCH_BOT_SCOPES: &[&str] = &[
    "user:write:chat",
    "user:manage:whispers",
    "moderator:manage:announcements",
    "moderator:manage:shoutouts",
];

pub(crate) const DEVICE_CANCELLED_REASON: &str = "device authorization cancelled";
pub(crate) const DEVICE_DENIED_REASON: &str = "authorization denied by user";
pub(crate) const DEVICE_EXPIRED_REASON: &str = "device code expired";
pub(crate) const DEVICE_CODE_USED_REASON: &str = "device code expired or already used";

pub(crate) fn twitch_validate_endpoint(endpoints: &PlatformEndpoints) -> String {
    format!(
        "{}{TWITCH_VALIDATE_PATH}",
        endpoints.base_url(EndpointSurface::TwitchOAuth)
    )
}

pub(crate) fn twitch_revoke_endpoint(endpoints: &PlatformEndpoints) -> String {
    format!(
        "{}{TWITCH_REVOKE_PATH}",
        endpoints.base_url(EndpointSurface::TwitchOAuth)
    )
}

pub(crate) fn twitch_device_endpoint(endpoints: &PlatformEndpoints) -> String {
    format!(
        "{}{TWITCH_DEVICE_PATH}",
        endpoints.base_url(EndpointSurface::TwitchOAuth)
    )
}

pub(crate) fn twitch_token_endpoint(endpoints: &PlatformEndpoints) -> String {
    format!(
        "{}{TWITCH_TOKEN_PATH}",
        endpoints.base_url(EndpointSurface::TwitchOAuth)
    )
}

pub fn twitch_auth_flow(endpoints: &PlatformEndpoints) -> AuthFlow {
    AuthFlow::DeviceCode {
        user_code_endpoint: twitch_device_endpoint(endpoints),
        token_endpoint: twitch_token_endpoint(endpoints),
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
    pub broadcaster_type: BroadcasterTier,
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
    pub refresh_token: Option<OAuthToken>,
    pub user_info: UserInfo,
    pub client_id: String,
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

#[derive(Deserialize)]
struct HelixUsers {
    data: Vec<HelixUser>,
}

#[derive(Deserialize)]
struct HelixUser {
    id: String,
    login: String,
    display_name: String,
    #[serde(default)]
    broadcaster_type: String,
}

#[derive(Clone)]
pub struct TwitchSignIn {
    oauth: TwitchOAuthClient,
    helix: Gateway,
}

impl TwitchSignIn {
    pub fn new(oauth: TwitchOAuthClient, helix: Gateway) -> Self {
        Self { oauth, helix }
    }

    pub fn broadcaster_flow(&self) -> TwitchAuthFlow {
        TwitchAuthFlow::new(self.oauth.clone(), self.helix.clone())
    }
}

pub struct TwitchAuthFlow {
    oauth: TwitchOAuthClient,
    helix: Gateway,
    scopes: &'static [&'static str],
    device_code: Option<String>,
    poll_interval: Duration,
    expires_at: Option<SystemTime>,
}

impl TwitchAuthFlow {
    pub fn new(oauth: TwitchOAuthClient, helix: Gateway) -> Self {
        Self {
            oauth,
            helix,
            scopes: TWITCH_BROADCASTER_SCOPES,
            device_code: None,
            poll_interval: DEFAULT_POLL_INTERVAL,
            expires_at: None,
        }
    }

    pub(crate) fn for_bot(oauth: TwitchOAuthClient, helix: Gateway) -> Self {
        let mut flow = Self::new(oauth, helix);
        flow.scopes = TWITCH_BOT_SCOPES;
        flow
    }

    pub async fn start(&mut self) -> Result<DeviceCodeInfo, PlatformError> {
        let authorization = self.oauth.request_device_code(self.scopes).await?;
        let expires_at = SystemTime::now() + Duration::from_secs(authorization.expires_in);
        self.device_code = Some(authorization.device_code);
        self.poll_interval =
            Duration::from_secs(authorization.interval.max(MIN_POLL_INTERVAL_SECS));
        self.expires_at = Some(expires_at);
        info!(
            scopes = self.scopes.len(),
            expires_in_secs = authorization.expires_in,
            poll_interval_secs = self.poll_interval.as_secs(),
            "twitch device authorization started"
        );
        Ok(DeviceCodeInfo {
            user_code: authorization.user_code,
            verification_uri: authorization.verification_uri,
            expires_at,
        })
    }

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

        let grant = loop {
            if SystemTime::now() >= expires_at {
                return Err(PlatformError::Auth {
                    reason: DEVICE_EXPIRED_REASON.into(),
                });
            }
            tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    return Err(PlatformError::Auth {
                        reason: DEVICE_CANCELLED_REASON.into(),
                    });
                }
                () = tokio::time::sleep(self.poll_interval) => {}
            }
            match self.oauth.poll_device_token(&device_code, &cancel).await? {
                DevicePoll::Pending => {
                    trace!("twitch device authorization still pending");
                }
                DevicePoll::SlowDown => {
                    self.poll_interval += SLOW_DOWN_INCREMENT;
                    debug!(
                        poll_interval_secs = self.poll_interval.as_secs(),
                        "twitch asked us to slow the device-code polling"
                    );
                }
                DevicePoll::Granted(grant) => break grant,
            }
        };

        info!("twitch device authorization granted; validating token");
        self.finish(grant, &cancel).await
    }

    async fn finish(
        &self,
        grant: TokenGrant,
        cancel: &CancellationToken,
    ) -> Result<TwitchAuthBundle, PlatformError> {
        let access_token = OAuthToken::new(grant.access_token);
        let validated = self
            .oauth
            .validate(&access_token, cancel)
            .await
            .map_err(|e| step_failed("token validate", e))?;
        if validated.client_id != self.oauth.client_id() {
            return Err(PlatformError::Auth {
                reason: "token client_id does not match configured client_id".into(),
            });
        }

        let user_info = self
            .fetch_user_info(&access_token, &validated, cancel)
            .await?;
        let expires_at = grant
            .expires_in
            .filter(|secs| *secs > 0)
            .or(Some(validated.expires_in).filter(|secs| *secs > 0))
            .map(|secs| SystemTime::now() + Duration::from_secs(secs));

        Ok(TwitchAuthBundle {
            access_token,
            refresh_token: grant.refresh_token.map(OAuthToken::new),
            user_info,
            client_id: self.oauth.client_id().to_owned(),
            expires_at,
        })
    }

    async fn fetch_user_info(
        &self,
        access_token: &OAuthToken,
        validated: &ValidatedToken,
        cancel: &CancellationToken,
    ) -> Result<UserInfo, PlatformError> {
        let request = GatewayRequest::get(HELIX_USERS_PATH)
            .label(HELIX_USERS_PATH)
            .header(CLIENT_ID_HEADER, self.oauth.client_id())
            .bearer(Secret::new(access_token.expose()))
            .query("id", validated.user_id.as_str())
            .cancel_on(cancel.clone());
        let response = self
            .helix
            .send(request)
            .await
            .map_err(|failure| step_failed("helix get_user", oauth_error(failure)))?;
        let users: HelixUsers = decode(&response).map_err(|e| step_failed("helix get_user", e))?;
        let user = users
            .data
            .into_iter()
            .next()
            .ok_or_else(|| PlatformError::Auth {
                reason: "helix returned empty user list".into(),
            })?;
        Ok(UserInfo {
            id: user.id,
            login: user.login,
            display_name: user.display_name,
            broadcaster_type: BroadcasterTier::from_helix_str(&user.broadcaster_type),
        })
    }
}

fn step_failed(step: &str, error: PlatformError) -> PlatformError {
    match error {
        PlatformError::Auth { .. } => error,
        other => PlatformError::Auth {
            reason: format!("{step} failed: {other}"),
        },
    }
}

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::Arc;

    use forge_events::EventPublisher;
    use tokio::sync::Notify;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    use super::*;
    use crate::bot_account::test_support::RecordingBus;
    use crate::sub_actions::test_support::{endpoints_with, helix_gateway_at, oauth_client_at};

    const CLIENT: &str = "test_client";
    const ACCESS: &str = "ACCESS_SENTINEL_r7";
    const USER_ID: &str = "4242";
    const HOLD: Duration = Duration::from_secs(60);
    const QUICK_POLL: Duration = Duration::from_millis(5);

    fn flow_at(server: &MockServer) -> TwitchAuthFlow {
        TwitchAuthFlow::new(
            oauth_client_at(&server.uri(), CLIENT),
            helix_gateway_at(&server.uri()),
        )
    }

    fn armed(mut flow: TwitchAuthFlow) -> TwitchAuthFlow {
        flow.device_code = Some("DEV123".to_owned());
        flow.expires_at = Some(SystemTime::now() + Duration::from_secs(3600));
        flow.poll_interval = QUICK_POLL;
        flow
    }

    fn grant(expires_in: Option<u64>) -> TokenGrant {
        TokenGrant {
            access_token: ACCESS.to_owned(),
            refresh_token: Some("REFRESH".to_owned()),
            expires_in,
        }
    }

    fn validate_body(client_id: &str, expires_in: Option<u64>) -> serde_json::Value {
        let mut body = serde_json::json!({
            "client_id": client_id,
            "login": "streamer",
            "user_id": USER_ID,
            "scopes": [],
        });
        if let Some(secs) = expires_in {
            body["expires_in"] = secs.into();
        }
        body
    }

    fn users_body() -> serde_json::Value {
        serde_json::json!({ "data": [{
            "id": USER_ID,
            "login": "streamer",
            "display_name": "Streamer",
            "broadcaster_type": "partner",
        }]})
    }

    async fn mount(server: &MockServer, route: &str, response: impl Respond + 'static) {
        Mock::given(path(route))
            .respond_with(response)
            .mount(server)
            .await;
    }

    async fn mount_device(server: &MockServer, expires_in: u64, interval: u64) {
        mount(
            server,
            "/device",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "device_code": "DEV123",
                "user_code": "WXYZ-1234",
                "verification_uri": "https://www.twitch.tv/activate",
                "expires_in": expires_in,
                "interval": interval,
            })),
        )
        .await;
    }

    async fn sign_in_server(validate: ResponseTemplate, users: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        mount(&server, "/validate", validate).await;
        mount(&server, HELIX_USERS_PATH, users).await;
        server
    }

    async fn finish_against(
        server: &MockServer,
        grant: TokenGrant,
    ) -> Result<TwitchAuthBundle, PlatformError> {
        flow_at(server)
            .finish(grant, &CancellationToken::new())
            .await
    }

    async fn first_body(server: &MockServer) -> String {
        let reqs = server.received_requests().await.unwrap();
        String::from_utf8_lossy(&reqs[0].body).into_owned()
    }

    #[tokio::test]
    async fn start_requests_scopes_and_parses_device_code_response() {
        let server = MockServer::start().await;
        mount_device(&server, 1800, 5).await;
        let mut flow = flow_at(&server);

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
        mount(
            &server,
            "/device",
            ResponseTemplate::new(400).set_body_string("bad client"),
        )
        .await;

        let err = flow_at(&server).start().await.unwrap_err();

        assert!(matches!(err, PlatformError::Http { status: 400, .. }));
    }

    #[tokio::test]
    async fn wait_before_start_reports_auth_error() {
        let mut flow = TwitchAuthFlow::new(
            crate::sub_actions::test_support::unreachable_oauth_client(CLIENT),
            crate::sub_actions::test_support::unreachable_helix_gateway(),
        );

        let err = flow
            .wait_for_authorization(CancellationToken::new())
            .await
            .unwrap_err();

        assert!(matches!(err, PlatformError::Auth { .. }));
    }

    #[tokio::test]
    async fn wait_cancelled_before_first_poll_returns_promptly_without_polling() {
        let server = MockServer::start().await;
        mount_device(&server, 3600, 5).await;
        mount(&server, "/token", ResponseTemplate::new(200)).await;
        let mut flow = flow_at(&server);
        flow.start().await.unwrap();

        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = flow.wait_for_authorization(cancel).await.unwrap_err();

        let PlatformError::Auth { reason } = err else {
            panic!("expected Auth error");
        };
        assert_eq!(reason, DEVICE_CANCELLED_REASON);
        let polled = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path() == "/token");
        assert!(!polled, "cancellation must abort before any token poll");
    }

    #[tokio::test]
    async fn wait_polls_again_after_authorization_pending_until_terminal() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(serde_json::json!({ "message": "authorization_pending" })),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;
        mount(
            &server,
            "/token",
            ResponseTemplate::new(400)
                .set_body_json(serde_json::json!({ "message": "access_denied" })),
        )
        .await;
        let mut flow = armed(flow_at(&server));

        let err = flow
            .wait_for_authorization(CancellationToken::new())
            .await
            .unwrap_err();

        assert!(matches!(err, PlatformError::Auth { .. }));
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            2,
            "loop must poll again after authorization_pending",
        );
    }

    #[tokio::test]
    async fn finish_identifies_the_validated_user_through_the_helix_users_lookup() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/validate"))
            .and(header("Authorization", format!("OAuth {ACCESS}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(validate_body(CLIENT, None)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(HELIX_USERS_PATH))
            .and(query_param("id", USER_ID))
            .and(header("Client-Id", CLIENT))
            .and(header("Authorization", format!("Bearer {ACCESS}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(users_body()))
            .mount(&server)
            .await;

        let bundle = finish_against(&server, grant(None)).await.unwrap();

        assert_eq!(
            (
                bundle.user_info.id.as_str(),
                bundle.user_info.login.as_str(),
                bundle.user_info.display_name.as_str(),
                bundle.user_info.broadcaster_type,
            ),
            (USER_ID, "streamer", "Streamer", BroadcasterTier::Partner)
        );
    }

    #[tokio::test]
    async fn finish_failure_reason_names_the_step_that_failed() {
        let ok_validate = || ResponseTemplate::new(200).set_body_json(validate_body(CLIENT, None));
        let ok_users = || ResponseTemplate::new(200).set_body_json(users_body());
        let cases = [
            (
                ResponseTemplate::new(401).set_body_string("invalid access token"),
                ok_users(),
                "token validate failed: HTTP 401",
            ),
            (
                ResponseTemplate::new(200).set_body_string("not json"),
                ok_users(),
                "token validate failed: malformed response",
            ),
            (
                ResponseTemplate::new(200).set_body_json(validate_body("other_client", None)),
                ok_users(),
                "token client_id does not match configured client_id",
            ),
            (
                ok_validate(),
                ResponseTemplate::new(500).set_body_string(format!("Bearer {ACCESS} rejected")),
                "helix get_user failed: HTTP 500",
            ),
            (
                ok_validate(),
                ResponseTemplate::new(200).set_body_string("not json"),
                "helix get_user failed: malformed response",
            ),
            (
                ok_validate(),
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
                "helix returned empty user list",
            ),
        ];

        for (validate, users, expected) in cases {
            let server = sign_in_server(validate, users).await;

            let Err(PlatformError::Auth { reason }) = finish_against(&server, grant(None)).await
            else {
                panic!("{expected}: finishing must fail with an Auth error");
            };

            assert!(reason.starts_with(expected), "{expected}: got {reason}");
            assert!(
                !reason.contains(ACCESS),
                "{expected}: token leaked into {reason}"
            );
        }
    }

    #[tokio::test]
    async fn finish_expiry_prefers_the_grant_lifetime_then_the_validated_one() {
        const GRANT_SECS: u64 = 3600;
        const VALIDATE_SECS: u64 = 7200;
        let cases = [
            (Some(GRANT_SECS), Some(VALIDATE_SECS), Some(GRANT_SECS)),
            (Some(0), Some(VALIDATE_SECS), Some(VALIDATE_SECS)),
            (None, Some(VALIDATE_SECS), Some(VALIDATE_SECS)),
            (Some(0), Some(0), None),
            (None, None, None),
        ];

        for (granted, validated, expected) in cases {
            let server = sign_in_server(
                ResponseTemplate::new(200).set_body_json(validate_body(CLIENT, validated)),
                ResponseTemplate::new(200).set_body_json(users_body()),
            )
            .await;
            let before = SystemTime::now();

            let bundle = finish_against(&server, grant(granted)).await.unwrap();

            let lifetime = bundle
                .expires_at
                .map(|at| at.duration_since(before).unwrap().as_secs());
            assert!(
                match (lifetime, expected) {
                    (Some(got), Some(want)) => got == want || got == want + 1,
                    (None, None) => true,
                    _ => false,
                },
                "grant {granted:?} validate {validated:?}: lifetime {lifetime:?}, want {expected:?}"
            );
        }
    }

    struct HeldAnswer(Arc<Notify>);

    impl Respond for HeldAnswer {
        fn respond(&self, _: &Request) -> ResponseTemplate {
            self.0.notify_one();
            ResponseTemplate::new(200).set_delay(HOLD)
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum InFlight {
        TokenPoll,
        Validate,
        HelixUsers,
    }

    #[tokio::test]
    async fn cancelling_aborts_the_in_flight_sign_in_call_without_publishing_a_failure() {
        for stage in [
            InFlight::TokenPoll,
            InFlight::Validate,
            InFlight::HelixUsers,
        ] {
            let server = MockServer::start().await;
            let reached = Arc::new(Notify::new());
            let held = || HeldAnswer(Arc::clone(&reached));
            let token = ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": ACCESS,
                "expires_in": 3600,
            }));
            let validate = ResponseTemplate::new(200).set_body_json(validate_body(CLIENT, None));
            match stage {
                InFlight::TokenPoll => mount(&server, "/token", held()).await,
                InFlight::Validate => {
                    mount(&server, "/token", token).await;
                    mount(&server, "/validate", held()).await;
                }
                InFlight::HelixUsers => {
                    mount(&server, "/token", token).await;
                    mount(&server, "/validate", validate).await;
                    mount(&server, HELIX_USERS_PATH, held()).await;
                }
            }
            let bus = Arc::new(RecordingBus::default());
            let helix = crate::helix_gateway::helix_gateway(
                &endpoints_with(&[(EndpointSurface::TwitchApi, server.uri().as_str())]),
                Arc::new(forge_net::TokenBucketRateLimiter::new(
                    crate::builtin::HELIX_BUDGET_CAPACITY,
                    crate::builtin::HELIX_BUDGET_WINDOW,
                )),
                Arc::clone(&bus) as Arc<dyn EventPublisher>,
            )
            .unwrap();
            let mut flow = armed(TwitchAuthFlow::new(
                oauth_client_at(&server.uri(), CLIENT),
                helix,
            ));
            let cancel = CancellationToken::new();

            let (outcome, ()) = tokio::join!(flow.wait_for_authorization(cancel.clone()), async {
                reached.notified().await;
                cancel.cancel();
            });

            let Err(PlatformError::Auth { reason }) = outcome else {
                panic!("{stage:?}: a cancelled sign-in must end with an Auth error");
            };
            assert_eq!(reason, DEVICE_CANCELLED_REASON, "{stage:?}");
            assert!(
                bus.of_kind("request.fail").is_empty(),
                "{stage:?}: a cancelled call must not publish a failure"
            );
        }
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
        let AuthFlow::DeviceCode { scopes, .. } =
            twitch_auth_flow(&crate::endpoints_test_support::unreachable_endpoints())
        else {
            panic!("twitch must use the device-code auth flow");
        };
        assert!(!scopes.is_empty(), "device grant must request scopes");
    }
}
