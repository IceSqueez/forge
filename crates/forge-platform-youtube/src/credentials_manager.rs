use std::sync::Arc;
use std::time::UNIX_EPOCH;

use forge_platform_core::PlatformError;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use time::{Duration, OffsetDateTime};

use crate::auth::{GoogleAuthFlow, YoutubeAuthBundle};
use crate::credentials::{CREDENTIAL_KEY, YoutubeCredentials};

const PLATFORM: &str = "youtube";
const REFRESH_BUFFER: Duration = Duration::minutes(5);

pub struct YoutubeCredentialsManager {
    repo: Arc<dyn CredentialsRepo>,
    google: GoogleAuthFlow,
}

impl YoutubeCredentialsManager {
    pub fn new(repo: Arc<dyn CredentialsRepo>, google: GoogleAuthFlow) -> Self {
        Self { repo, google }
    }

    /// Returns `None` if no credentials row exists for this account.
    pub async fn load(&self) -> Result<Option<YoutubeCredentials>, PlatformError> {
        let key = CredentialId::new(CREDENTIAL_KEY);
        let Some(json) = self.repo.load(&key).await.map_err(storage_err)? else {
            return Ok(None);
        };
        let creds: YoutubeCredentials = serde_json::from_str(&json)?;
        Ok(Some(creds))
    }

    pub async fn save_from_bundle(&self, bundle: YoutubeAuthBundle) -> Result<(), PlatformError> {
        let unix_secs = bundle
            .expires_at
            .duration_since(UNIX_EPOCH)
            .map_err(|e| PlatformError::Auth {
                reason: e.to_string(),
            })?
            .as_secs();
        let expires_at = OffsetDateTime::from_unix_timestamp(unix_secs as i64).map_err(|e| {
            PlatformError::Auth {
                reason: e.to_string(),
            }
        })?;
        let creds = YoutubeCredentials {
            access_token: bundle.access_token.expose().to_owned(),
            refresh_token: bundle.refresh_token.expose().to_owned(),
            client_id: bundle.client_id,
            channel_id: bundle.channel_id,
            channel_title: bundle.channel_title,
            expires_at,
        };
        self.persist(&creds).await
    }

    /// Returns a valid access token, refreshing proactively when within 5 minutes of expiry.
    ///
    /// Returns `Err(PlatformError::ReauthRequired)` when no credentials are stored or when
    /// the upstream refresh fails with `invalid_grant`.
    pub async fn get_valid_access_token(&self) -> Result<String, PlatformError> {
        let creds = self.load().await?.ok_or_else(reauth_err)?;
        if creds.expires_at <= OffsetDateTime::now_utc() + REFRESH_BUFFER {
            let refreshed = self.refresh(&creds.refresh_token).await?;
            return Ok(refreshed.access_token);
        }
        Ok(creds.access_token)
    }

    /// Issues a token-refresh request directly.
    ///
    /// When the Google response omits `refresh_token`, the previously stored token is
    /// preserved. Returns `Err(PlatformError::ReauthRequired)` on `invalid_grant`.
    pub async fn refresh(&self, refresh_token: &str) -> Result<YoutubeCredentials, PlatformError> {
        let existing = self.load().await?.ok_or_else(reauth_err)?;

        let response = self
            .google
            .http_client()
            .post(self.google.token_endpoint())
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", existing.client_id.as_str()),
                ("client_secret", self.google.client_secret()),
            ])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?;

        let status = response.status().as_u16();

        if status == 400 {
            let body = response.text().await.unwrap_or_default();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body)
                && val.get("error").and_then(|v| v.as_str()) == Some("invalid_grant")
            {
                return Err(reauth_err());
            }
            return Err(PlatformError::Http { status, body });
        }

        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(PlatformError::Http { status, body });
        }

        let body = response.text().await.map_err(|e| PlatformError::Network {
            reason: e.to_string(),
        })?;
        let parsed: RefreshResponse = serde_json::from_str(&body)?;

        let updated = YoutubeCredentials {
            access_token: parsed.access_token,
            refresh_token: parsed
                .refresh_token
                .unwrap_or_else(|| refresh_token.to_owned()),
            client_id: existing.client_id,
            channel_id: existing.channel_id,
            channel_title: existing.channel_title,
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

    async fn persist(&self, creds: &YoutubeCredentials) -> Result<(), PlatformError> {
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

#[derive(serde::Deserialize)]
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
    use std::time::{Duration as StdDuration, SystemTime};

    use async_trait::async_trait;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use forge_types::OAuthToken;
    use serde_json::json;
    use time::{Duration, OffsetDateTime};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::YoutubeCredentialsManager;
    use crate::auth::{GoogleAuthFlow, YoutubeAuthBundle};
    use crate::credentials::YoutubeCredentials;

    struct InMemRepo(Mutex<HashMap<String, String>>);

    impl InMemRepo {
        fn empty() -> Arc<Self> {
            Arc::new(Self(Mutex::new(HashMap::new())))
        }

        fn with_creds(creds: &YoutubeCredentials) -> Arc<Self> {
            let json = serde_json::to_string(creds).unwrap();
            let mut map = HashMap::new();
            map.insert(crate::credentials::CREDENTIAL_KEY.to_owned(), json);
            Arc::new(Self(Mutex::new(map)))
        }

        fn get_stored_creds(&self) -> Option<YoutubeCredentials> {
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

    fn stub_creds(expires_at: OffsetDateTime) -> YoutubeCredentials {
        YoutubeCredentials {
            access_token: "ya29.existing".to_owned(),
            refresh_token: "1//existing_refresh".to_owned(),
            client_id: "test_client_id".to_owned(),
            channel_id: "UCtest123".to_owned(),
            channel_title: "Test Channel".to_owned(),
            expires_at,
        }
    }

    fn manager_with_server(
        repo: Arc<dyn CredentialsRepo>,
        server: &MockServer,
    ) -> YoutubeCredentialsManager {
        let google = GoogleAuthFlow::with_endpoints(
            "test_cid".to_owned(),
            "test_secret".to_owned(),
            format!("{}/device", server.uri()),
            format!("{}/token", server.uri()),
            format!("{}/channels", server.uri()),
        );
        YoutubeCredentialsManager::new(repo, google)
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
        assert_eq!(loaded.channel_id, "UCtest123");
        assert_eq!(loaded.access_token, "ya29.existing");
    }

    #[tokio::test]
    async fn save_from_bundle_persists_credentials() {
        let server = MockServer::start().await;
        let repo = InMemRepo::empty();
        let mgr = manager_with_server(repo.clone(), &server);

        let bundle = YoutubeAuthBundle {
            access_token: OAuthToken::new("ya29.bundle"),
            refresh_token: OAuthToken::new("1//bundle_refresh"),
            channel_id: "UCbundle".to_owned(),
            channel_title: "Bundle Channel".to_owned(),
            client_id: "bundle_cid".to_owned(),
            expires_at: SystemTime::now() + StdDuration::from_secs(3600),
        };
        mgr.save_from_bundle(bundle).await.unwrap();

        let stored = repo.get_stored_creds().unwrap();
        assert_eq!(stored.access_token, "ya29.bundle");
        assert_eq!(stored.refresh_token, "1//bundle_refresh");
        assert_eq!(stored.channel_id, "UCbundle");
        assert_eq!(stored.client_id, "bundle_cid");
    }

    #[tokio::test]
    async fn get_valid_access_token_returns_existing_when_fresh() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));
        let mgr = manager_with_server(InMemRepo::with_creds(&creds), &server);

        let token = mgr.get_valid_access_token().await.unwrap();
        assert_eq!(token, "ya29.existing");

        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "token endpoint must not be called for fresh credentials",
        );
    }

    #[tokio::test]
    async fn get_valid_access_token_refreshes_when_within_5min() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::seconds(60));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "ya29.refreshed",
                "expires_in": 3599,
                "token_type": "Bearer"
            })))
            .mount(&server)
            .await;

        let repo = InMemRepo::with_creds(&creds);
        let mgr = manager_with_server(repo.clone(), &server);

        let token = mgr.get_valid_access_token().await.unwrap();
        assert_eq!(token, "ya29.refreshed");

        let stored = repo.get_stored_creds().unwrap();
        assert_eq!(stored.access_token, "ya29.refreshed");
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
    async fn refresh_handles_missing_refresh_token_in_response() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "ya29.new_access",
                "expires_in": 3599,
                "token_type": "Bearer"
            })))
            .mount(&server)
            .await;

        let repo = InMemRepo::with_creds(&creds);
        let mgr = manager_with_server(repo.clone(), &server);

        mgr.refresh("1//existing_refresh").await.unwrap();

        let stored = repo.get_stored_creds().unwrap();
        assert_eq!(
            stored.refresh_token, "1//existing_refresh",
            "old refresh token must be preserved when response omits it",
        );
        assert_eq!(stored.access_token, "ya29.new_access");
    }

    #[tokio::test]
    async fn refresh_returns_reauth_required_on_invalid_grant() {
        let server = MockServer::start().await;
        let creds = stub_creds(OffsetDateTime::now_utc() + Duration::hours(1));

        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({ "error": "invalid_grant" })),
            )
            .mount(&server)
            .await;

        let mgr = manager_with_server(InMemRepo::with_creds(&creds), &server);

        let err = mgr.refresh("1//expired_refresh").await.unwrap_err();
        assert!(
            matches!(
                err,
                forge_platform_core::PlatformError::ReauthRequired { .. }
            ),
            "expected ReauthRequired on invalid_grant, got: {err}",
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
