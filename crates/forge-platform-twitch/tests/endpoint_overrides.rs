#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_net::TokenBucketRateLimiter;
use forge_platform_core::{AuthFlow, EndpointSurface, PlatformEndpoints};
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_platform_twitch::{
    BroadcasterTier, HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW, TWITCH_OAUTH_BUDGET_CAPACITY,
    TWITCH_OAUTH_BUDGET_WINDOW, TwitchAuthFlow, TwitchCredentialsManager, TwitchOAuthClient,
    helix_gateway, twitch_auth_flow, twitch_oauth_gateway,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::OAuthToken;
use serde_json::json;
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[derive(Default)]
struct MemoryRepo {
    rows: Mutex<BTreeMap<String, String>>,
}

#[async_trait]
impl CredentialsRepo for MemoryRepo {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        self.rows
            .lock()
            .unwrap()
            .insert(id.to_string(), plaintext_bundle.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.rows.lock().unwrap().get(&id.to_string()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.rows.lock().unwrap().remove(&id.to_string()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(Vec::new())
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

fn overrides(oauth_base: String, api_base: String) -> PlatformEndpoints {
    PlatformEndpoints::resolve(move |variable| {
        if variable == EndpointSurface::TwitchOAuth.env_var() {
            Some(OsString::from(oauth_base.clone()))
        } else if variable == EndpointSurface::TwitchApi.env_var() {
            Some(OsString::from(api_base.clone()))
        } else {
            None
        }
    })
    .unwrap()
}

fn oauth_override(base: String) -> PlatformEndpoints {
    overrides(base, "http://127.0.0.1:9".to_owned())
}

fn oauth_client(endpoints: &PlatformEndpoints) -> TwitchOAuthClient {
    let limiter = Arc::new(TokenBucketRateLimiter::new(
        TWITCH_OAUTH_BUDGET_CAPACITY,
        TWITCH_OAUTH_BUDGET_WINDOW,
    ));
    TwitchOAuthClient::new(
        twitch_oauth_gateway(endpoints, limiter).unwrap(),
        "client".to_owned(),
    )
}

struct DiscardingBus;

impl EventPublisher for DiscardingBus {
    fn publish(&self, _: Event) {}
}

fn helix(endpoints: &PlatformEndpoints) -> forge_net::Gateway {
    helix_gateway(
        endpoints,
        Arc::new(TokenBucketRateLimiter::new(
            HELIX_BUDGET_CAPACITY,
            HELIX_BUDGET_WINDOW,
        )),
        Arc::new(DiscardingBus),
    )
    .unwrap()
}

async fn received_routes(server: &MockServer) -> Vec<(String, String)> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect()
}

fn routes(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(verb, route)| ((*verb).to_owned(), (*route).to_owned()))
        .collect()
}

#[tokio::test]
async fn oauth_override_carries_the_token_refresh_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fresh-access",
            "refresh_token": "fresh-refresh",
            "expires_in": 3600
        })))
        .mount(&server)
        .await;
    let endpoints = oauth_override(format!("{}/oauth2", server.uri()));
    let repo = Arc::new(MemoryRepo::default());
    store_credential(
        repo.as_ref(),
        &StoredCredential {
            access_token: OAuthToken::new("stale-access".to_owned()),
            refresh_token: Some(OAuthToken::new("stale-refresh".to_owned())),
            user_id: "7".to_owned(),
            login: "streamer".to_owned(),
            expires_at: Some(SystemTime::now()),
        },
    )
    .await
    .unwrap();
    let manager = TwitchCredentialsManager::new(oauth_client(&endpoints), repo);

    let token = manager.get_valid_access_token().await.unwrap();

    assert_eq!(token.expose(), "fresh-access");
}

#[tokio::test]
async fn overrides_carry_the_whole_sign_in_through_the_oauth_and_helix_hosts() {
    let server = MockServer::start().await;
    let answers = [
        (
            "/oauth2/device",
            json!({
                "device_code": "DEV123",
                "user_code": "WXYZ-1234",
                "verification_uri": "https://www.twitch.tv/activate",
                "expires_in": 600,
                "interval": 1
            }),
        ),
        (
            "/oauth2/token",
            json!({ "access_token": "granted-access", "expires_in": 3600 }),
        ),
        (
            "/oauth2/validate",
            json!({ "client_id": "client", "login": "streamer", "user_id": "7", "scopes": [] }),
        ),
        (
            "/helix/users",
            json!({ "data": [{
                "id": "7",
                "login": "streamer",
                "display_name": "Streamer",
                "broadcaster_type": "affiliate"
            }]}),
        ),
    ];
    for (route, body) in answers {
        Mock::given(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    let endpoints = overrides(format!("{}/oauth2", server.uri()), server.uri());
    let mut flow = TwitchAuthFlow::new(oauth_client(&endpoints), helix(&endpoints));

    flow.start().await.unwrap();
    let bundle = flow
        .wait_for_authorization(CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(
        bundle.user_info.broadcaster_type,
        BroadcasterTier::Affiliate
    );
    assert_eq!(
        received_routes(&server).await,
        routes(&[
            ("POST", "/oauth2/device"),
            ("POST", "/oauth2/token"),
            ("GET", "/oauth2/validate"),
            ("GET", "/helix/users"),
        ]),
    );
}

#[test]
fn oauth_override_carries_the_advertised_device_flow_endpoints() {
    let endpoints = oauth_override("http://127.0.0.1:9/oauth2".to_owned());

    let AuthFlow::DeviceCode {
        user_code_endpoint,
        token_endpoint,
        ..
    } = twitch_auth_flow(&endpoints)
    else {
        panic!("twitch must advertise the device-code flow");
    };

    assert_eq!(
        (user_code_endpoint.as_str(), token_endpoint.as_str()),
        (
            "http://127.0.0.1:9/oauth2/device",
            "http://127.0.0.1:9/oauth2/token"
        ),
    );
}
