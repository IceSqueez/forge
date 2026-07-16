use std::sync::Arc;

use forge_platform_core::PlatformError;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use serde::Deserialize;
use time::{Duration, OffsetDateTime};

use crate::auth::KickAuthBundle;
use crate::credentials::{CREDENTIAL_KEY, KickCredentials};

const PLATFORM: &str = "kick";
const REFRESH_BUFFER: Duration = Duration::minutes(5);

pub struct KickCredentialsManager {
    repo: Arc<dyn CredentialsRepo>,
    client: reqwest::Client,
    client_id: String,
    refresh_endpoint: String,
}

impl KickCredentialsManager {
    pub fn new(repo: Arc<dyn CredentialsRepo>, client: reqwest::Client, client_id: String) -> Self {
        Self {
            repo,
            client,
            client_id,
            refresh_endpoint: crate::auth::KICK_TOKEN_ENDPOINT.to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_refresh_endpoint(mut self, url: String) -> Self {
        self.refresh_endpoint = url;
        self
    }

    /// Returns `None` if no credentials row exists for this account.
    pub async fn load(&self) -> Result<Option<KickCredentials>, PlatformError> {
        let key = CredentialId::new(CREDENTIAL_KEY);
        let Some(json) = self.repo.load(&key).await.map_err(storage_err)? else {
            return Ok(None);
        };
        let creds: KickCredentials = serde_json::from_str(&json)?;
        Ok(Some(creds))
    }

    pub async fn save_from_bundle(&self, bundle: KickAuthBundle) -> Result<(), PlatformError> {
        let creds = KickCredentials {
            access_token: bundle.access_token,
            refresh_token: bundle.refresh_token,
            user_id: bundle.user_id,
            username: bundle.username,
            client_id: bundle.client_id,
            expires_at: bundle.expires_at,
        };
        self.persist(&creds).await
    }

    /// Returns a valid access token, refreshing proactively when within 5 minutes of expiry.
    ///
    /// Returns `Err(PlatformError::ReauthRequired)` when no credentials are stored or when
    /// the upstream refresh fails.
    pub async fn get_valid_access_token(&self) -> Result<String, PlatformError> {
        let creds = self.load().await?.ok_or_else(reauth_err)?;
        if creds.expires_at <= OffsetDateTime::now_utc() + REFRESH_BUFFER {
            let refreshed = self.refresh(&creds.refresh_token).await?;
            return Ok(refreshed.access_token);
        }
        Ok(creds.access_token)
    }

    /// Public-client form POST - no `client_secret`. Returns
    /// `Err(PlatformError::ReauthRequired)` on 400 or 401 from the upstream.
    pub async fn refresh(&self, refresh_token: &str) -> Result<KickCredentials, PlatformError> {
        let existing = self.load().await?.ok_or_else(reauth_err)?;

        let response = self
            .client
            .post(&self.refresh_endpoint)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", self.client_id.as_str()),
            ])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            if status == 400 || status == 401 {
                return Err(reauth_err());
            }
            return Err(PlatformError::Http { status, body });
        }

        let body_text = response.text().await.map_err(|e| PlatformError::Network {
            reason: e.to_string(),
        })?;
        let parsed: RefreshResponse = serde_json::from_str(&body_text)?;

        let updated = KickCredentials {
            access_token: parsed.access_token,
            refresh_token: parsed
                .refresh_token
                .unwrap_or_else(|| refresh_token.to_owned()),
            user_id: existing.user_id,
            username: existing.username,
            client_id: existing.client_id,
            expires_at: OffsetDateTime::now_utc() + Duration::seconds(parsed.expires_in as i64),
        };
        self.persist(&updated).await?;
        Ok(updated)
    }

    pub async fn clear(&self) -> Result<(), PlatformError> {
        self.repo
            .delete(&CredentialId::new(CREDENTIAL_KEY))
            .await
            .map_err(storage_err)?;
        Ok(())
    }

    async fn persist(&self, creds: &KickCredentials) -> Result<(), PlatformError> {
        let json = serde_json::to_string(creds)?;
        self.repo
            .store(&CredentialId::new(CREDENTIAL_KEY), &json)
            .await
            .map_err(storage_err)
    }
}

fn storage_err(e: StorageError) -> PlatformError {
    PlatformError::Io(std::io::Error::other(e))
}

fn reauth_err() -> PlatformError {
    PlatformError::ReauthRequired {
        platform: PLATFORM.to_owned(),
    }
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use serde_json::json;
    use time::{Duration, OffsetDateTime};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::KickCredentialsManager;
    use crate::auth::KickAuthBundle;
    use crate::credentials::KickCredentials;

    struct InMemRepo(Mutex<HashMap<String, String>>);

    impl InMemRepo {
        fn empty() -> Arc<Self> {
            Arc::new(Self(Mutex::new(HashMap::new())))
        }

        fn with_creds(creds: &KickCredentials) -> Arc<Self> {
            let json = serde_json::to_string(creds).unwrap();
            let mut map = HashMap::new();
            map.insert(crate::credentials::CREDENTIAL_KEY.to_owned(), json);
            Arc::new(Self(Mutex::new(map)))
        }

        fn get_stored_creds(&self) -> Option<KickCredentials> {
            let guard = self.0.lock().unwrap();
            guard
                .get(crate::credentials::CREDENTIAL_KEY)
                .and_then(|s| serde_json::from_str(s).ok())
        }
    }

    #[async_trait]
    impl CredentialsRepo for InMemRepo {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), plaintext.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .keys()
                .map(|k| CredentialId::new(k.clone()))
                .collect())
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

    fn stub_creds(expires_at: OffsetDateTime) -> KickCredentials {
        KickCredentials {
            access_token: "kick_access_existing".to_owned(),
            refresh_token: "kick_refresh_existing".to_owned(),
            user_id: 42,
            username: "streamer".to_owned(),
            client_id: "test_cid".to_owned(),
            expires_at,
        }
    }

    fn manager_with_server(
        repo: Arc<dyn CredentialsRepo>,
        server: &MockServer,
    ) -> KickCredentialsManager {
        KickCredentialsManager::new(repo, reqwest::Client::new(), "test_cid".to_owned())
            .with_refresh_endpoint(format!("{}/token", server.uri()))
    }

    #[tokio::test]
    async fn load_returns_none_when_no_row() {
        let server = MockServer::start().await;
        let mgr = manager_with_server(InMemRepo::empty(), &server);
        let result = mgr.load().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn load_returns_some_when_row_present() {
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));
        let server = MockServer::start().await;
        let mgr = manager_with_server(InMemRepo::with_creds(&creds), &server);
        let loaded = mgr.load().await.unwrap().unwrap();
        assert_eq!(loaded.user_id, 42);
        assert_eq!(loaded.access_token, "kick_access_existing");
    }

    #[tokio::test]
    async fn save_from_bundle_persists_credentials() {
        let server = MockServer::start().await;
        let repo = InMemRepo::empty();
        let mgr = manager_with_server(repo.clone(), &server);

        let bundle = KickAuthBundle {
            access_token: "new_access".to_owned(),
            refresh_token: "new_refresh".to_owned(),
            user_id: 99,
            username: "bundle_streamer".to_owned(),
            client_id: "bundle_cid".to_owned(),
            expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
        };
        mgr.save_from_bundle(bundle).await.unwrap();

        let stored = repo.get_stored_creds().unwrap();
        assert_eq!(stored.access_token, "new_access");
        assert_eq!(stored.refresh_token, "new_refresh");
        assert_eq!(stored.user_id, 99);
        assert_eq!(stored.username, "bundle_streamer");
    }

    #[tokio::test]
    async fn get_valid_access_token_returns_existing_when_fresh() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));
        let mgr = manager_with_server(InMemRepo::with_creds(&creds), &server);

        let token = mgr.get_valid_access_token().await.unwrap();
        assert_eq!(token, "kick_access_existing");

        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "refresh endpoint must not be called for fresh credentials",
        );
    }

    #[tokio::test]
    async fn get_valid_access_token_refreshes_when_within_5min() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::seconds(60));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "refreshed_access",
                "refresh_token": "refreshed_refresh",
                "expires_in": 86400
            })))
            .mount(&server)
            .await;

        let repo = InMemRepo::with_creds(&creds);
        let mgr = manager_with_server(repo.clone(), &server);

        let token = mgr.get_valid_access_token().await.unwrap();
        assert_eq!(token, "refreshed_access");

        let stored = repo.get_stored_creds().unwrap();
        assert_eq!(stored.access_token, "refreshed_access");
    }

    #[tokio::test]
    async fn get_valid_access_token_returns_reauth_required_when_no_creds() {
        let server = MockServer::start().await;
        let mgr = manager_with_server(InMemRepo::empty(), &server);

        let err = mgr.get_valid_access_token().await.unwrap_err();
        assert!(
            matches!(
                err,
                forge_platform_core::PlatformError::ReauthRequired { .. }
            ),
            "expected ReauthRequired, got: {err}",
        );
    }

    #[tokio::test]
    async fn refresh_sends_form_with_client_id_no_secret() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));

        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("client_id=test_cid"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new_access",
                "refresh_token": "new_refresh",
                "expires_in": 86400
            })))
            .mount(&server)
            .await;

        let repo = InMemRepo::with_creds(&creds);
        let mgr = manager_with_server(repo.clone(), &server);
        mgr.refresh("kick_refresh_existing").await.unwrap();

        let reqs = server.received_requests().await.unwrap();
        assert_eq!(reqs.len(), 1);
        let body = std::str::from_utf8(&reqs[0].body).unwrap();
        assert!(
            !body.contains("client_secret"),
            "client_secret must not appear in form body - public client flow"
        );
    }

    #[tokio::test]
    async fn refresh_handles_missing_refresh_token_in_response() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new_access_only",
                "expires_in": 86400
            })))
            .mount(&server)
            .await;

        let repo = InMemRepo::with_creds(&creds);
        let mgr = manager_with_server(repo.clone(), &server);
        mgr.refresh("kick_refresh_existing").await.unwrap();

        let stored = repo.get_stored_creds().unwrap();
        assert_eq!(
            stored.refresh_token, "kick_refresh_existing",
            "old refresh token must be preserved when response omits it",
        );
        assert_eq!(stored.access_token, "new_access_only");
    }

    #[tokio::test]
    async fn refresh_returns_reauth_required_on_400() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"})),
            )
            .mount(&server)
            .await;

        let mgr = manager_with_server(InMemRepo::with_creds(&creds), &server);
        let err = mgr.refresh("expired_refresh").await.unwrap_err();
        assert!(
            matches!(
                err,
                forge_platform_core::PlatformError::ReauthRequired { .. }
            ),
            "expected ReauthRequired on 400, got: {err}",
        );
    }

    #[tokio::test]
    async fn refresh_returns_reauth_required_on_401() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let mgr = manager_with_server(InMemRepo::with_creds(&creds), &server);
        let err = mgr.refresh("expired_refresh").await.unwrap_err();
        assert!(
            matches!(
                err,
                forge_platform_core::PlatformError::ReauthRequired { .. }
            ),
            "expected ReauthRequired on 401, got: {err}",
        );
    }

    #[tokio::test]
    async fn clear_removes_credentials_row() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));
        let repo = InMemRepo::with_creds(&creds);
        let mgr = manager_with_server(repo.clone(), &server);

        mgr.clear().await.unwrap();

        assert!(repo.get_stored_creds().is_none());
    }
}
