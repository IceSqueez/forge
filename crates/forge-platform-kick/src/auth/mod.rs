pub(crate) mod credentials;
pub(crate) mod credentials_manager;
pub(crate) mod oauth_client;

use std::sync::Arc;
use std::time::Duration;

use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::auth::{CALLBACK_PATH, PkceAuthorizeConfig, PkceLoopback};
use forge_platform_core::{AuthFlow, PlatformError};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::auth::credentials_manager::KickCredentialsManager;
use crate::auth::oauth_client::KickOAuthClient;
use crate::gateway::{authorized, gateway_error, malformed};

pub const KICK_AUTHORIZE_ENDPOINT: &str = "https://id.kick.com/oauth/authorize";
pub const KICK_TOKEN_ENDPOINT: &str = "https://id.kick.com/oauth/token";

pub const KICK_OAUTH_CALLBACK_PORT: u16 = 53127;

const KICK_USERS_PATH: &str = "/users";
const EMPTY_USERS: &str = "users endpoint returned empty data array";
const NO_REFRESH_TOKEN: &str = "Kick token exchange did not return a refresh_token";
const LIFETIME_OUT_OF_RANGE: &str = "token lifetime out of range";

const KICK_SCOPES: &[&str] = &[
    "user:read",
    "channel:read",
    "channel:write",
    "channel:rewards:read",
    "channel:rewards:write",
    "chat:write",
    "moderation:chat_message:manage",
    "moderation:ban",
];

pub fn kick_auth_flow() -> AuthFlow {
    AuthFlow::LocalCallback {
        authorize_url: KICK_AUTHORIZE_ENDPOINT.to_owned(),
        token_endpoint: KICK_TOKEN_ENDPOINT.to_owned(),
        redirect_path: CALLBACK_PATH.to_owned(),
        scopes: KICK_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[derive(Debug, Clone)]
pub struct LoopbackCode {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub struct KickAuthBundle {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: u64,
    pub username: String,
    pub client_id: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Clone)]
pub struct KickSignIn {
    manager: Arc<KickCredentialsManager>,
}

impl KickSignIn {
    pub fn new(manager: Arc<KickCredentialsManager>) -> Self {
        Self { manager }
    }

    pub fn flow(&self) -> KickAuthFlow {
        KickAuthFlow::new(
            self.manager.oauth_client().clone(),
            self.manager.api().clone(),
        )
    }

    pub async fn save(&self, bundle: KickAuthBundle) -> Result<(), PlatformError> {
        self.manager.save_from_bundle(bundle).await
    }
}

pub struct KickAuthFlow {
    oauth: KickOAuthClient,
    api: Gateway,
    loopback: PkceLoopback,
}

impl KickAuthFlow {
    pub fn new(oauth: KickOAuthClient, api: Gateway) -> Self {
        Self::with_callback_port(oauth, api, Some(KICK_OAUTH_CALLBACK_PORT))
    }

    pub fn with_callback_port(
        oauth: KickOAuthClient,
        api: Gateway,
        preferred_port: Option<u16>,
    ) -> Self {
        let loopback = PkceLoopback::new(PkceAuthorizeConfig {
            client_id: oauth.client_id().to_owned(),
            authorize_endpoint: oauth.authorize_endpoint().to_owned(),
            scopes: KICK_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
            authorize_pre_redirect_params: vec![("redirect".to_owned(), "127.0.0.1".to_owned())],
            authorize_trailing_params: Vec::new(),
            preferred_port,
        });
        Self {
            oauth,
            api,
            loopback,
        }
    }

    pub async fn start(&mut self) -> Result<LoopbackCode, PlatformError> {
        let url = self.loopback.start(&[]).await?;
        Ok(LoopbackCode {
            auth_url: url.auth_url,
        })
    }

    pub async fn wait_for_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<KickAuthBundle, PlatformError> {
        let authorization = self.loopback.await_authorization(timeout).await?;
        let grant = self.oauth.exchange_code(&authorization).await?;
        let refresh_token = grant.refresh_token.ok_or_else(|| PlatformError::Auth {
            reason: NO_REFRESH_TOKEN.to_owned(),
        })?;
        let expires_at = expiry_after(grant.expires_in)?;

        let user = fetch_user_info(&self.api, &grant.access_token).await?;

        Ok(KickAuthBundle {
            access_token: grant.access_token,
            refresh_token,
            user_id: user.user_id,
            username: user.name,
            client_id: self.oauth.client_id().to_owned(),
            expires_at,
        })
    }
}

pub(crate) fn expiry_after(expires_in: Option<u64>) -> Result<OffsetDateTime, PlatformError> {
    let lifetime = i64::try_from(expires_in.unwrap_or(0)).unwrap_or(i64::MAX);
    OffsetDateTime::now_utc()
        .checked_add(time::Duration::seconds(lifetime))
        .ok_or_else(|| malformed(LIFETIME_OUT_OF_RANGE))
}

#[derive(Debug, Deserialize)]
struct UserRecord {
    user_id: u64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct UsersResponse {
    data: Vec<UserRecord>,
}

async fn fetch_user_info(api: &Gateway, access_token: &str) -> Result<UserRecord, PlatformError> {
    let request = authorized(
        GatewayRequest::get(KICK_USERS_PATH),
        access_token,
        KICK_USERS_PATH,
    );
    let response = api.send(request).await.map_err(gateway_error)?;
    let parsed: UsersResponse = response.json().map_err(malformed)?;
    parsed
        .data
        .into_iter()
        .next()
        .ok_or_else(|| PlatformError::Http {
            status: response.status(),
            body: EMPTY_USERS.to_owned(),
        })
}

pub fn client_credentials() -> Option<(String, String)> {
    let client_id = resolve_credential(
        std::env::var("FORGE_KICK_CLIENT_ID").ok().as_deref(),
        option_env!("FORGE_KICK_CLIENT_ID"),
    )?;
    let client_secret = resolve_credential(
        std::env::var("FORGE_KICK_CLIENT_SECRET").ok().as_deref(),
        option_env!("FORGE_KICK_CLIENT_SECRET"),
    )?;
    Some((client_id, client_secret))
}

fn resolve_credential(
    runtime_env: Option<&str>,
    compile_env: Option<&'static str>,
) -> Option<String> {
    runtime_env
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| compile_env.filter(|s| !s.is_empty()).map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_platform_core::{EndpointSurface, PlatformEndpoints};
    use reqwest::StatusCode;
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::endpoints_test_support::{
        ExhaustedLimiter, GrantingLimiter, OAUTH_CLIENT_SECRET, RecordingPublisher, api_at,
        api_limited_by, api_with, endpoints_at, oauth_client_at, oauth_client_over,
        unreachable_api, unreachable_oauth_client,
    };

    const ACCESS_TOKEN: &str = "kick-access-token-secret-5d";
    const REQUEST_FAIL_KIND: &str = "request.fail";

    static CALLBACK_PORT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn query_param(auth_url: &str, key: &str) -> String {
        reqwest::Url::parse(auth_url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == key)
            .unwrap()
            .1
            .into_owned()
    }

    fn deliver_callback(auth_url: &str, code: &str) {
        let url = format!(
            "{}?code={code}&state={}",
            query_param(auth_url, "redirect_uri"),
            query_param(auth_url, "state")
        );
        tokio::spawn(async move { reqwest::Client::new().get(&url).send().await });
    }

    async fn routes_of(server: &MockServer) -> Vec<(String, String)> {
        server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .map(|r| (r.method.to_string(), r.url.path().to_owned()))
            .collect()
    }

    async fn mount(server: &MockServer, verb: &str, route: &str, reply: ResponseTemplate) {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(reply)
            .mount(server)
            .await;
    }

    fn token_grant(body: serde_json::Value) -> ResponseTemplate {
        ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_json(body)
    }

    fn one_user() -> ResponseTemplate {
        ResponseTemplate::new(StatusCode::OK.as_u16())
            .set_body_json(json!({ "data": [{ "user_id": 7, "name": "streamer" }] }))
    }

    async fn sign_in_against(
        oauth: &MockServer,
        api: &MockServer,
    ) -> Result<KickAuthBundle, PlatformError> {
        let endpoints = endpoints_at(&[
            (EndpointSurface::KickOAuth, &oauth.uri()),
            (EndpointSurface::KickPublicApi, &api.uri()),
        ]);
        let mut flow = KickAuthFlow::with_callback_port(
            oauth_client_over(&endpoints, Arc::new(GrantingLimiter)),
            api_at(&api.uri()),
            None,
        );
        let auth_url = flow.start().await.unwrap().auth_url;
        assert!(
            auth_url.starts_with(&format!("{}/authorize?", oauth.uri())),
            "{auth_url}"
        );
        deliver_callback(&auth_url, "code123");
        flow.wait_for_authorization(Duration::from_secs(30)).await
    }

    #[tokio::test]
    async fn sign_in_sends_the_authorize_page_and_token_post_to_the_oauth_host_and_users_to_the_api_host()
     {
        let oauth = MockServer::start().await;
        let api = MockServer::start().await;
        mount(
            &oauth,
            "POST",
            "/token",
            token_grant(
                json!({ "access_token": "acc", "refresh_token": "ref", "expires_in": 3600 }),
            ),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .and(header("authorization", "Bearer acc"))
            .respond_with(one_user())
            .mount(&api)
            .await;

        let bundle = sign_in_against(&oauth, &api).await.unwrap();

        assert_eq!(
            (
                bundle.user_id,
                bundle.username.as_str(),
                bundle.refresh_token.as_str(),
                routes_of(&oauth).await,
                routes_of(&api).await
            ),
            (
                7,
                "streamer",
                "ref",
                vec![("POST".to_owned(), "/token".to_owned())],
                vec![("GET".to_owned(), "/users".to_owned())]
            )
        );
    }

    #[tokio::test]
    async fn a_grant_without_a_refresh_token_fails_the_sign_in_before_the_user_lookup() {
        let oauth = MockServer::start().await;
        let api = MockServer::start().await;
        mount(
            &oauth,
            "POST",
            "/token",
            token_grant(json!({ "access_token": "acc", "expires_in": 3600 })),
        )
        .await;
        mount(&api, "GET", "/users", one_user()).await;

        let outcome = sign_in_against(&oauth, &api).await;

        assert!(
            matches!(outcome, Err(PlatformError::Auth { .. })) && routes_of(&api).await.is_empty(),
            "{:?}",
            outcome.err()
        );
    }

    #[tokio::test]
    async fn a_grant_lifetime_past_the_calendar_fails_the_sign_in_as_malformed() {
        let oauth = MockServer::start().await;
        let api = MockServer::start().await;
        mount(
            &oauth,
            "POST",
            "/token",
            token_grant(
                json!({ "access_token": "acc", "refresh_token": "ref", "expires_in": u64::MAX }),
            ),
        )
        .await;
        mount(&api, "GET", "/users", one_user()).await;

        let outcome = sign_in_against(&oauth, &api).await;

        assert!(
            matches!(outcome, Err(PlatformError::MalformedResponse { .. })),
            "{:?}",
            outcome.err()
        );
    }

    #[test]
    fn expiry_after_accepts_representable_lifetimes_and_rejects_overflow() {
        let before = OffsetDateTime::now_utc();
        let in_an_hour = expiry_after(Some(3600)).unwrap();
        let missing = expiry_after(None).unwrap();
        let after = OffsetDateTime::now_utc();
        assert!(
            in_an_hour >= before + time::Duration::hours(1)
                && in_an_hour <= after + time::Duration::hours(1)
                && missing >= before
                && missing <= after
        );
        for overflowing in [u64::MAX, i64::MAX as u64, i64::MAX as u64 + 1] {
            assert!(
                matches!(
                    expiry_after(Some(overflowing)),
                    Err(PlatformError::MalformedResponse { .. })
                ),
                "{overflowing}"
            );
        }
    }

    #[test]
    fn kick_auth_flow_yields_local_callback_variant() {
        let flow = kick_auth_flow();
        let AuthFlow::LocalCallback {
            authorize_url,
            token_endpoint,
            redirect_path,
            scopes,
        } = flow
        else {
            unreachable!("kick_auth_flow must return LocalCallback variant");
        };
        assert_eq!(authorize_url, KICK_AUTHORIZE_ENDPOINT);
        assert_eq!(token_endpoint, KICK_TOKEN_ENDPOINT);
        assert_eq!(redirect_path, CALLBACK_PATH);
        for required in [
            "chat:write",
            "moderation:ban",
            "moderation:chat_message:manage",
            "channel:write",
            "channel:rewards:read",
            "channel:rewards:write",
        ] {
            assert!(
                scopes.iter().any(|s| s == required),
                "requested scopes must contain {required}"
            );
        }
    }

    #[test]
    fn client_credentials_prefer_a_non_empty_runtime_value_then_a_non_empty_compile_value() {
        for (runtime, compile, expected) in [
            (Some("runtime_id"), Some("compile_id"), Some("runtime_id")),
            (None, Some("compile_id"), Some("compile_id")),
            (Some(""), Some("compile_id"), Some("compile_id")),
            (None, Some(""), None),
            (Some(""), None, None),
            (None, None, None),
        ] {
            assert_eq!(
                resolve_credential(runtime, compile).as_deref(),
                expected,
                "{runtime:?} / {compile:?}"
            );
        }
    }

    #[tokio::test]
    async fn start_builds_authorize_url_with_kick_endpoint_and_redirect_quirk() {
        let _port = CALLBACK_PORT_LOCK.lock().await;
        let defaults = PlatformEndpoints::default();
        let mut flow = KickAuthFlow::new(
            oauth_client_over(&defaults, Arc::new(GrantingLimiter)),
            unreachable_api(),
        );
        let url = flow.start().await.unwrap().auth_url;
        let redirect_pos = url.find("redirect=127.0.0.1").unwrap();
        let redirect_uri_pos = url.find("redirect_uri=").unwrap();
        assert!(
            url.starts_with(KICK_AUTHORIZE_ENDPOINT)
                && !url.contains(OAUTH_CLIENT_SECRET)
                && redirect_pos < redirect_uri_pos
                && reqwest::Url::parse(&query_param(&url, "redirect_uri"))
                    .unwrap()
                    .port()
                    == Some(KICK_OAUTH_CALLBACK_PORT),
            "{url}"
        );
    }

    #[tokio::test]
    async fn sign_in_reuses_the_shared_oauth_client_and_api_gateway() {
        let oauth = MockServer::start().await;
        let api = MockServer::start().await;
        mount(&api, "GET", "/users", one_user()).await;
        let manager = Arc::new(KickCredentialsManager::new(
            Arc::new(forge_storage::credentials::MockCredentialsRepo::new()),
            oauth_client_at(&oauth.uri()),
            api_at(&api.uri()),
        ));
        let flow = KickSignIn::new(manager).flow();

        let user = fetch_user_info(&flow.api, ACCESS_TOKEN).await.unwrap();

        assert_eq!(
            (user.user_id, flow.oauth.authorize_endpoint().to_owned()),
            (7, format!("{}/authorize", oauth.uri()))
        );
    }

    #[tokio::test]
    async fn fetch_user_info_sends_the_bearer_and_resolves_the_first_user() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/users"))
            .and(header("authorization", format!("Bearer {ACCESS_TOKEN}")))
            .respond_with(
                ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_json(json!({
                    "data": [{ "user_id": 99, "name": "kick_streamer" }, { "user_id": 1, "name": "other" }]
                })),
            )
            .mount(&server)
            .await;

        let user = fetch_user_info(&api_at(&server.uri()), ACCESS_TOKEN)
            .await
            .unwrap();

        assert_eq!((user.user_id, user.name.as_str()), (99, "kick_streamer"));
    }

    enum Lookup {
        Http(u16),
        RateLimited,
        Malformed,
    }

    #[tokio::test]
    async fn fetch_user_info_maps_each_users_answer_to_its_platform_error() {
        for (label, reply, expected) in [
            (
                "401 stays Http 401",
                ResponseTemplate::new(StatusCode::UNAUTHORIZED.as_u16()),
                Lookup::Http(StatusCode::UNAUTHORIZED.as_u16()),
            ),
            (
                "429",
                ResponseTemplate::new(StatusCode::TOO_MANY_REQUESTS.as_u16()),
                Lookup::RateLimited,
            ),
            (
                "empty data on 200",
                ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_json(json!({ "data": [] })),
                Lookup::Http(StatusCode::OK.as_u16()),
            ),
            (
                "empty data on 203 carries 203",
                ResponseTemplate::new(StatusCode::NON_AUTHORITATIVE_INFORMATION.as_u16())
                    .set_body_json(json!({ "data": [] })),
                Lookup::Http(StatusCode::NON_AUTHORITATIVE_INFORMATION.as_u16()),
            ),
            (
                "undecodable 200",
                ResponseTemplate::new(StatusCode::OK.as_u16()).set_body_string("<html/>"),
                Lookup::Malformed,
            ),
        ] {
            let server = MockServer::start().await;
            mount(&server, "GET", "/users", reply).await;

            let error = fetch_user_info(&api_at(&server.uri()), ACCESS_TOKEN)
                .await
                .unwrap_err();

            let mapped = match (&error, &expected) {
                (PlatformError::Http { status, .. }, Lookup::Http(expected)) => status == expected,
                (PlatformError::RateLimited { .. }, Lookup::RateLimited) => true,
                (PlatformError::MalformedResponse { .. }, Lookup::Malformed) => true,
                _ => false,
            };
            assert!(mapped, "{label}: {error:?}");
        }
    }

    #[tokio::test]
    async fn an_exhausted_api_budget_refuses_the_users_lookup_without_a_request() {
        let server = MockServer::start().await;
        mount(&server, "GET", "/users", one_user()).await;

        let outcome = fetch_user_info(
            &api_limited_by(&server.uri(), Arc::new(ExhaustedLimiter)),
            ACCESS_TOKEN,
        )
        .await;

        assert!(
            matches!(outcome, Err(PlatformError::RateLimitExhausted))
                && routes_of(&server).await.is_empty(),
            "{:?}",
            outcome.err()
        );
    }

    #[tokio::test]
    async fn a_refused_users_lookup_publishes_request_fail_labelled_users_without_the_token() {
        let server = MockServer::start().await;
        mount(
            &server,
            "GET",
            "/users",
            ResponseTemplate::new(StatusCode::UNAUTHORIZED.as_u16())
                .set_body_string(format!("bad token {ACCESS_TOKEN}")),
        )
        .await;
        let bus = Arc::new(RecordingPublisher::default());
        let api = api_with(&server.uri(), Arc::new(GrantingLimiter), bus.clone());

        let error = fetch_user_info(&api, ACCESS_TOKEN).await.unwrap_err();

        let failures = bus.events_of_kind(REQUEST_FAIL_KIND);
        assert_eq!(failures.len(), 1);
        assert_eq!(
            (
                failures[0].payload["endpoint"].as_str(),
                failures[0].payload["status_code"].as_u64(),
                failures[0].payload.to_string().contains(ACCESS_TOKEN),
                format!("{error} {error:?}").contains(ACCESS_TOKEN)
            ),
            (Some("/users"), Some(401), false, false)
        );
    }

    #[tokio::test]
    async fn a_sign_in_over_an_unreachable_oauth_host_maps_to_network() {
        let mut flow =
            KickAuthFlow::with_callback_port(unreachable_oauth_client(), unreachable_api(), None);
        let auth_url = flow.start().await.unwrap().auth_url;
        deliver_callback(&auth_url, "code123");

        let outcome = flow.wait_for_authorization(Duration::from_secs(30)).await;

        assert!(
            matches!(outcome, Err(PlatformError::Network { .. })),
            "{:?}",
            outcome.err()
        );
    }
}
