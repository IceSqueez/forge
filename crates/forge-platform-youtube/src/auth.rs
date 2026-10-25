use std::sync::Arc;
use std::time::{Duration, SystemTime};

use forge_net::{GatewayRequest, Secret};
use forge_platform_core::auth::{CALLBACK_PATH, PkceAuthorizeConfig, PkceLoopback};
use forge_platform_core::{AuthFlow, PlatformError};
use forge_types::OAuthToken;
use serde::Deserialize;

use crate::credentials_manager::YoutubeCredentialsManager;
use crate::data_api::{YoutubeDataApi, malformed};
use crate::oauth_client::GoogleOAuthClient;

pub const GOOGLE_AUTHORIZE_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

const YOUTUBE_CHANNELS_PATH: &str = "/channels";
const OWN_CHANNEL_COST: u32 = 1;
const OWN_CHANNEL_MISSING_SCOPE: &str = "channel identity scope missing";
const NO_CHANNEL_FOUND: &str = "no channel found";
const NO_CHANNEL_STATUS: u16 = forge_net::status::OK;

pub const YOUTUBE_BROADCASTER_SCOPES: &[&str] =
    &["https://www.googleapis.com/auth/youtube.force-ssl"];

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
    pub channel_handle: Option<String>,
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

#[derive(Clone)]
pub struct YoutubeSignIn {
    manager: Arc<YoutubeCredentialsManager>,
}

impl YoutubeSignIn {
    pub fn new(manager: Arc<YoutubeCredentialsManager>) -> Self {
        Self { manager }
    }

    pub fn flow(&self) -> GoogleAuthFlow {
        GoogleAuthFlow::new(
            self.manager.oauth_client().clone(),
            self.manager.data_api().clone(),
        )
    }

    pub async fn save(&self, bundle: YoutubeAuthBundle) -> Result<(), PlatformError> {
        self.manager.save_from_bundle(bundle).await
    }
}

pub struct GoogleAuthFlow {
    oauth: GoogleOAuthClient,
    api: YoutubeDataApi,
    force_consent: bool,
    loopback: PkceLoopback,
}

impl GoogleAuthFlow {
    pub fn new(oauth: GoogleOAuthClient, api: YoutubeDataApi) -> Self {
        let loopback = PkceLoopback::new(PkceAuthorizeConfig {
            client_id: oauth.client_id().to_owned(),
            authorize_endpoint: GOOGLE_AUTHORIZE_ENDPOINT.to_owned(),
            scopes: YOUTUBE_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            authorize_pre_redirect_params: Vec::new(),
            authorize_trailing_params: vec![("access_type".to_owned(), "offline".to_owned())],
            preferred_port: None,
        });
        Self {
            oauth,
            api,
            force_consent: false,
            loopback,
        }
    }

    pub fn set_force_consent(&mut self, force: bool) {
        self.force_consent = force;
    }

    pub async fn start(&mut self) -> Result<LoopbackCode, PlatformError> {
        let extra: &[(&str, &str)] = if self.force_consent {
            &[("prompt", "consent")]
        } else {
            &[]
        };
        let url = self.loopback.start(extra).await?;
        Ok(LoopbackCode {
            auth_url: url.auth_url,
        })
    }

    pub async fn wait_for_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<YoutubeAuthBundle, PlatformError> {
        let authorization = self.loopback.await_authorization(timeout).await?;
        let grant = self.oauth.exchange_code(&authorization).await?;
        let refresh_token = grant.refresh_token.ok_or_else(|| PlatformError::Auth {
            reason: "Google did not return a refresh_token; retry with consent prompt".into(),
        })?;
        let expires_at = SystemTime::now()
            .checked_add(Duration::from_secs(grant.expires_in.unwrap_or(0)))
            .ok_or_else(|| malformed("token lifetime out of range"))?;

        let channel = fetch_own_channel(&self.api, &grant.access_token)
            .await?
            .ok_or_else(|| PlatformError::Http {
                status: NO_CHANNEL_STATUS,
                body: NO_CHANNEL_FOUND.to_owned(),
            })?;

        Ok(YoutubeAuthBundle {
            access_token: OAuthToken::new(grant.access_token),
            refresh_token: OAuthToken::new(refresh_token),
            channel_id: channel.id,
            channel_title: channel.snippet.title,
            channel_handle: channel.snippet.custom_url,
            client_id: self.oauth.client_id().to_owned(),
            expires_at,
        })
    }
}

pub(crate) async fn fetch_own_channel(
    api: &YoutubeDataApi,
    access_token: &str,
) -> Result<Option<ChannelItem>, PlatformError> {
    let request = GatewayRequest::get(YOUTUBE_CHANNELS_PATH)
        .label(YOUTUBE_CHANNELS_PATH)
        .bearer(Secret::new(access_token))
        .query("part", "snippet")
        .query("mine", "true");
    let channels: ChannelsListResponse = api
        .send(OWN_CHANNEL_COST, request)
        .await
        .map_err(|e| e.into_platform_error(OWN_CHANNEL_MISSING_SCOPE))?
        .json()
        .map_err(malformed)?;
    Ok(channels.items.into_iter().next())
}

#[derive(Deserialize)]
pub(crate) struct ChannelsListResponse {
    #[serde(default)]
    pub(crate) items: Vec<ChannelItem>,
}

#[derive(Deserialize)]
pub(crate) struct ChannelItem {
    pub(crate) id: String,
    pub(crate) snippet: ChannelSnippet,
}

#[derive(Deserialize)]
pub(crate) struct ChannelSnippet {
    pub(crate) title: String,
    #[serde(default, rename = "customUrl")]
    pub(crate) custom_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsString;

    use forge_net::TokenBucketRateLimiter;
    use forge_platform_core::{EndpointSurface, PlatformEndpoints};
    use reqwest::StatusCode;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::endpoints_test_support::{
        OAUTH_CLIENT_ID, OAUTH_CLIENT_SECRET, RecordingPublisher, StubReply, StubServer,
        data_api_at, data_api_charging, data_api_with, oauth_client_at, unreachable_data_api,
        unreachable_oauth_client, used_today,
    };
    use crate::oauth_client::{
        GOOGLE_OAUTH_BUDGET_CAPACITY, GOOGLE_OAUTH_BUDGET_WINDOW, google_oauth_gateway,
    };
    use crate::quota_state::SharedQuota;

    const CALLBACK_DEADLINE: Duration = Duration::from_secs(30);
    const ACCESS_TOKEN: &str = "ya29.test_access";
    const REQUEST_FAIL_KIND: &str = "request.fail";

    fn overrides(pairs: &[(EndpointSurface, String)]) -> PlatformEndpoints {
        PlatformEndpoints::resolve(|variable| {
            pairs
                .iter()
                .find(|(surface, _)| surface.env_var() == variable)
                .map(|(_, base)| OsString::from(base))
        })
        .unwrap()
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

    fn token_success_body() -> serde_json::Value {
        json!({
            "access_token": ACCESS_TOKEN,
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

    async fn mount_token(server: &MockServer, status: StatusCode, body: serde_json::Value) {
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(status.as_u16()).set_body_json(body))
            .mount(server)
            .await;
    }

    async fn mount_channels(server: &MockServer, body: serde_json::Value) {
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    async fn sign_in(mut flow: GoogleAuthFlow) -> Result<YoutubeAuthBundle, PlatformError> {
        let auth_url = flow.start().await.unwrap().auth_url;
        trigger_callback(&auth_url, "auth_code_abc");
        flow.wait_for_authorization(CALLBACK_DEADLINE).await
    }

    fn query_of(url: &str) -> HashMap<String, String> {
        reqwest::Url::parse(url)
            .unwrap()
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    fn trigger_callback(auth_url: &str, code: &str) {
        let query = query_of(auth_url);
        let url = format!(
            "{}?code={code}&state={}",
            query["redirect_uri"], query["state"]
        );
        tokio::spawn(async move { reqwest::Client::new().get(&url).send().await.ok() });
    }

    #[tokio::test]
    async fn overrides_route_token_exchange_and_channel_lookup_to_their_own_hosts() {
        let oauth = MockServer::start().await;
        let api = MockServer::start().await;
        mount_token(&oauth, StatusCode::OK, token_success_body()).await;
        mount_channels(&api, channel_body()).await;
        let endpoints = overrides(&[
            (EndpointSurface::YouTubeOAuth, oauth.uri()),
            (EndpointSurface::YouTubeDataApi, api.uri()),
        ]);
        let gateway = google_oauth_gateway(
            &endpoints,
            Arc::new(TokenBucketRateLimiter::new(
                GOOGLE_OAUTH_BUDGET_CAPACITY,
                GOOGLE_OAUTH_BUDGET_WINDOW,
            )),
        )
        .unwrap();
        let flow = GoogleAuthFlow::new(
            GoogleOAuthClient::new(gateway, "cid".to_owned(), "csec".to_owned()),
            YoutubeDataApi::new(
                &endpoints,
                SharedQuota::default(),
                Arc::new(RecordingPublisher::default()),
            )
            .unwrap(),
        );

        sign_in(flow).await.unwrap();

        assert_eq!(
            (routes_of(&oauth).await, routes_of(&api).await),
            (
                vec![("POST".to_owned(), "/token".to_owned())],
                vec![("GET".to_owned(), "/channels".to_owned())]
            )
        );
    }

    #[tokio::test]
    async fn start_builds_the_google_authorize_url_for_offline_pkce_access() {
        let mut flow = GoogleAuthFlow::new(unreachable_oauth_client(), unreachable_data_api());

        let url = flow.start().await.unwrap().auth_url;

        let query = query_of(&url);
        assert_eq!(
            (
                url.starts_with(GOOGLE_AUTHORIZE_ENDPOINT),
                query.get("client_id").map(String::as_str),
                query.get("scope").map(String::as_str),
                query.get("access_type").map(String::as_str),
                query.get("code_challenge_method").map(String::as_str),
                query.contains_key("code_challenge"),
                query
                    .get("redirect_uri")
                    .map(|uri| uri.starts_with("http://127.0.0.1:")),
                query.contains_key("prompt"),
            ),
            (
                true,
                Some(OAUTH_CLIENT_ID),
                Some(YOUTUBE_BROADCASTER_SCOPES.join(" ").as_str()),
                Some("offline"),
                Some("S256"),
                true,
                Some(true),
                false,
            )
        );
    }

    #[tokio::test]
    async fn start_appends_prompt_when_force_consent_enabled() {
        let mut flow = GoogleAuthFlow::new(unreachable_oauth_client(), unreachable_data_api());
        flow.set_force_consent(true);

        let url = flow.start().await.unwrap().auth_url;

        assert_eq!(
            query_of(&url).get("prompt").map(String::as_str),
            Some("consent")
        );
    }

    #[tokio::test]
    async fn wait_for_authorization_exchanges_the_code_and_resolves_the_channel() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code=auth_code_abc"))
            .and(body_string_contains("code_verifier="))
            .and(body_string_contains(format!(
                "client_secret={OAUTH_CLIENT_SECRET}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(token_success_body()))
            .mount(&server)
            .await;
        mount_channels(&server, channel_body()).await;
        let flow = GoogleAuthFlow::new(oauth_client_at(&server.uri()), data_api_at(&server.uri()));

        let bundle = sign_in(flow).await.unwrap();

        assert_eq!(
            (
                bundle.access_token.expose(),
                bundle.channel_id.as_str(),
                bundle.channel_title.as_str(),
                bundle.client_id.as_str()
            ),
            (ACCESS_TOKEN, "UCtest123", "Test Channel", OAUTH_CLIENT_ID)
        );
    }

    #[tokio::test]
    async fn a_sign_in_charges_one_quota_unit_for_the_channel_lookup() {
        let server = MockServer::start().await;
        mount_token(&server, StatusCode::OK, token_success_body()).await;
        mount_channels(&server, channel_body()).await;
        let quota = SharedQuota::default();
        let flow = GoogleAuthFlow::new(
            oauth_client_at(&server.uri()),
            data_api_charging(&server.uri(), quota.clone()),
        );

        sign_in(flow).await.unwrap();

        assert_eq!(used_today(&quota).await, OWN_CHANNEL_COST);
    }

    #[tokio::test]
    async fn a_refused_code_exchange_never_reaches_the_channel_lookup_or_the_quota() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            StatusCode::BAD_REQUEST,
            json!({ "error": "invalid_grant" }),
        )
        .await;
        mount_channels(&server, channel_body()).await;
        let quota = SharedQuota::default();
        let flow = GoogleAuthFlow::new(
            oauth_client_at(&server.uri()),
            data_api_charging(&server.uri(), quota.clone()),
        );

        let outcome = sign_in(flow).await;

        assert_eq!(
            (
                outcome.is_err(),
                routes_of(&server).await,
                used_today(&quota).await
            ),
            (true, vec![("POST".to_owned(), "/token".to_owned())], 0)
        );
    }

    #[tokio::test]
    async fn token_response_without_refresh_token_returns_auth_error() {
        let server = MockServer::start().await;
        mount_token(
            &server,
            StatusCode::OK,
            json!({
                "access_token": "ya29.access_only",
                "expires_in": 3599,
                "token_type": "Bearer",
            }),
        )
        .await;
        mount_channels(&server, channel_body()).await;
        let flow = GoogleAuthFlow::new(oauth_client_at(&server.uri()), data_api_at(&server.uri()));

        let err = sign_in(flow).await.unwrap_err();

        assert!(
            matches!(err, PlatformError::Auth { ref reason } if reason.contains("refresh_token")),
            "expected Auth error mentioning refresh_token, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn a_sign_in_whose_account_has_no_channel_reports_no_channel_found() {
        for body in [
            json!({ "items": [] }),
            json!({ "kind": "youtube#channelListResponse" }),
        ] {
            let server = MockServer::start().await;
            mount_token(&server, StatusCode::OK, token_success_body()).await;
            mount_channels(&server, body.clone()).await;
            let flow =
                GoogleAuthFlow::new(oauth_client_at(&server.uri()), data_api_at(&server.uri()));

            let err = sign_in(flow).await.unwrap_err();

            assert!(
                matches!(&err, PlatformError::Http { status, body } if *status == StatusCode::OK.as_u16() && body == NO_CHANNEL_FOUND),
                "{body}: {err:?}"
            );
        }
    }

    fn channels_refusal(status: StatusCode, reason: &str) -> StubReply {
        StubReply::json(
            status.as_u16(),
            &json!({ "error": { "code": status.as_u16(), "errors": [{ "reason": reason }] } }),
        )
    }

    #[tokio::test]
    async fn own_channel_lookup_maps_each_refusal_to_its_platform_error() {
        for (label, reply, expected) in [
            (
                "403 quotaExceeded",
                channels_refusal(StatusCode::FORBIDDEN, "quotaExceeded"),
                "quota",
            ),
            (
                "403 insufficientPermissions",
                channels_refusal(StatusCode::FORBIDDEN, "insufficientPermissions"),
                "scope",
            ),
            (
                "undecodable 200",
                StubReply::text(StatusCode::OK.as_u16(), "<html>"),
                "malformed",
            ),
        ] {
            let server = StubServer::start().await;
            server.reply_always(YOUTUBE_CHANNELS_PATH, reply);

            let outcome = fetch_own_channel(&data_api_at(&server.uri()), ACCESS_TOKEN).await;

            let mapped = match &outcome {
                Err(PlatformError::QuotaExhausted) => "quota",
                Err(PlatformError::Auth { reason }) if reason == OWN_CHANNEL_MISSING_SCOPE => {
                    "scope"
                }
                Err(PlatformError::MalformedResponse { .. }) => "malformed",
                _ => "other",
            };
            assert_eq!(mapped, expected, "{label}: {:?}", outcome.err());
        }
    }

    #[tokio::test]
    async fn a_refused_own_channel_lookup_publishes_request_fail_and_stays_charged() {
        for reply in [
            channels_refusal(StatusCode::FORBIDDEN, "quotaExceeded"),
            channels_refusal(StatusCode::FORBIDDEN, "insufficientPermissions"),
            StubReply::status(StatusCode::INTERNAL_SERVER_ERROR.as_u16()),
        ] {
            let server = StubServer::start().await;
            server.reply_always(YOUTUBE_CHANNELS_PATH, reply);
            let bus = Arc::new(RecordingPublisher::default());
            let quota = SharedQuota::default();
            let api = data_api_with(&server.uri(), quota.clone(), bus.clone());

            fetch_own_channel(&api, ACCESS_TOKEN).await.err().unwrap();

            let events = bus.events_of_kind(REQUEST_FAIL_KIND);
            assert_eq!(
                (
                    events
                        .iter()
                        .map(|event| event.payload["endpoint"].clone())
                        .collect::<Vec<_>>(),
                    used_today(&quota).await
                ),
                (vec![json!(YOUTUBE_CHANNELS_PATH)], OWN_CHANNEL_COST)
            );
        }
    }
}
