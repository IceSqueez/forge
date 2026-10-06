#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use forge_platform_core::{AuthFlow, EndpointSurface, PlatformEndpoints, PlatformError};
use forge_platform_twitch::credentials::{StoredCredential, store_credential};
use forge_platform_twitch::{TwitchAuthFlow, TwitchCredentialsManager, twitch_auth_flow};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::OAuthToken;
use serde_json::json;
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const STEP_DEADLINE: Duration = Duration::from_secs(10);

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

fn oauth_override(base: String) -> PlatformEndpoints {
    PlatformEndpoints::resolve(move |variable| {
        (variable == EndpointSurface::TwitchOAuth.env_var()).then(|| OsString::from(base.clone()))
    })
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
    let manager = TwitchCredentialsManager::new(&endpoints, repo, "client".to_owned());

    let token = tokio::time::timeout(STEP_DEADLINE, manager.get_valid_access_token())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(token.expose(), "fresh-access");
}

#[tokio::test]
async fn oauth_override_carries_device_code_start_and_poll_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/device"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "DEV123",
            "user_code": "WXYZ-1234",
            "verification_uri": "https://www.twitch.tv/activate",
            "expires_in": 600,
            "interval": 1
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "message": "access_denied"
        })))
        .mount(&server)
        .await;
    let endpoints = oauth_override(format!("{}/oauth2", server.uri()));
    let mut flow = TwitchAuthFlow::new(&endpoints, "client".to_owned());

    flow.start().await.unwrap();
    let outcome = tokio::time::timeout(
        STEP_DEADLINE,
        flow.wait_for_authorization(CancellationToken::new()),
    )
    .await
    .unwrap();

    assert!(matches!(outcome, Err(PlatformError::Auth { .. })));
    assert_eq!(
        received_routes(&server).await,
        routes(&[("POST", "/oauth2/device"), ("POST", "/oauth2/token")]),
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
