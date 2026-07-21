use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_platform_core::auth::{
    PkceRefreshConfig, PkceRefresher, REFRESH_BUFFER_SECS, ReauthPolicy,
};
use forge_storage::{CredentialsRepo, StorageError};
use forge_types::OAuthToken;

use crate::auth::TWITCH_TOKEN_ENDPOINT;
use crate::credentials::{StoredCredential, load, store_credential};

const PLATFORM: &str = "twitch";
const REFRESH_BUFFER: Duration = Duration::from_secs(REFRESH_BUFFER_SECS);

pub struct TwitchCredentialsManager {
    repo: Arc<dyn CredentialsRepo>,
    refresher: PkceRefresher,
}

impl TwitchCredentialsManager {
    pub fn new(repo: Arc<dyn CredentialsRepo>, client_id: String) -> Self {
        Self::with_endpoint(repo, client_id, TWITCH_TOKEN_ENDPOINT.to_owned())
    }

    pub(crate) fn with_endpoint(
        repo: Arc<dyn CredentialsRepo>,
        client_id: String,
        refresh_endpoint: String,
    ) -> Self {
        let refresher = PkceRefresher::new(PkceRefreshConfig {
            platform: PLATFORM.to_owned(),
            client_id,
            client_secret: None,
            token_endpoint: refresh_endpoint,
            reauth_policy: ReauthPolicy::AnyClientError,
        });
        Self { repo, refresher }
    }

    pub async fn load(&self) -> Result<Option<StoredCredential>, PlatformError> {
        load(self.repo.as_ref()).await.map_err(storage_err)
    }

    /// Returns a valid access token, renewing proactively when within five
    /// minutes of expiry. A credential without a refresh token cannot be
    /// renewed, so a near-dead one routes to re-auth rather than handing back a
    /// token about to fail mid-request.
    pub async fn get_valid_access_token(&self) -> Result<OAuthToken, PlatformError> {
        let cred = self.load().await?.ok_or_else(reauth_err)?;
        let near_expiry = cred
            .expires_at
            .map(|at| at <= SystemTime::now() + REFRESH_BUFFER)
            .unwrap_or(false);
        if near_expiry {
            let refresh = cred.refresh_token.as_ref().ok_or_else(reauth_err)?;
            let renewed = self.refresh(refresh).await?;
            return Ok(renewed.access_token);
        }
        Ok(cred.access_token)
    }

    /// Public-client `grant_type=refresh_token` POST - no `client_secret`.
    /// Persists the rotated refresh token Twitch returns and invalidates the
    /// old one; the prior token is kept only when the response omits a new one.
    /// A 400/401 means the refresh token itself is rejected → re-auth.
    pub async fn refresh(
        &self,
        refresh_token: &OAuthToken,
    ) -> Result<StoredCredential, PlatformError> {
        let existing = self.load().await?.ok_or_else(reauth_err)?;
        let parsed = self.refresher.refresh(refresh_token.expose()).await?;

        let renewed = StoredCredential {
            access_token: OAuthToken::new(parsed.access_token),
            refresh_token: Some(
                parsed
                    .refresh_token
                    .map(OAuthToken::new)
                    .unwrap_or_else(|| refresh_token.clone()),
            ),
            user_id: existing.user_id,
            login: existing.login,
            expires_at: parsed
                .expires_in
                .filter(|secs| *secs > 0)
                .map(|secs| SystemTime::now() + Duration::from_secs(secs)),
        };
        store_credential(self.repo.as_ref(), &renewed)
            .await
            .map_err(storage_err)?;
        Ok(renewed)
    }
}

#[async_trait]
impl crate::helix::HelixTokenSource for TwitchCredentialsManager {
    async fn access_token(&self) -> Result<OAuthToken, crate::helix::HelixError> {
        self.get_valid_access_token().await.map_err(|e| match e {
            PlatformError::ReauthRequired { .. } => crate::helix::HelixError::ReauthRequired,
            PlatformError::Io(io) => crate::helix::HelixError::Credentials(io.to_string()),
            other => crate::helix::HelixError::Credentials(other.to_string()),
        })
    }
}

#[async_trait]
impl crate::helix::HelixTokenRefresher for TwitchCredentialsManager {
    async fn refresh(&self) -> Result<OAuthToken, crate::helix::HelixError> {
        let cred = self
            .load()
            .await
            .map_err(|_| crate::helix::HelixError::ReauthRequired)?
            .ok_or(crate::helix::HelixError::ReauthRequired)?;
        let refresh_token = cred
            .refresh_token
            .as_ref()
            .ok_or(crate::helix::HelixError::ReauthRequired)?;
        match self.refresh(refresh_token).await {
            Ok(renewed) => Ok(renewed.access_token),
            Err(_) => Err(crate::helix::HelixError::ReauthRequired),
        }
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::time::SystemTime;

    use async_trait::async_trait;
    use forge_platform_core::PlatformError;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use forge_types::OAuthToken;
    use serde_json::json;
    use time::OffsetDateTime;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::TwitchCredentialsManager;
    use crate::credentials::{StoredCredential, TWITCH_CREDENTIAL_ID};

    // ---------------------------------------------------------------------------
    // In-memory CredentialsRepo - same pattern as Kick harness.
    // ---------------------------------------------------------------------------

    struct InMemRepo(Mutex<HashMap<String, String>>);

    impl InMemRepo {
        fn empty() -> Arc<Self> {
            Arc::new(Self(Mutex::new(HashMap::new())))
        }

        /// Seed the repo with a credential by directly writing the JSON blob that
        /// `store_credential` would produce. Avoids `block_on` inside a tokio test.
        fn seeded(cred: &StoredCredential) -> Arc<Self> {
            let expires_at_unix: Option<i64> = cred.expires_at.and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs() as i64)
            });
            let blob = json!({
                "access_token": cred.access_token.expose(),
                "refresh_token": cred.refresh_token.as_ref().map(OAuthToken::expose),
                "user_id": cred.user_id,
                "login": cred.login,
                "expires_at_unix": expires_at_unix,
            })
            .to_string();
            let mut map = HashMap::new();
            map.insert(TWITCH_CREDENTIAL_ID.to_owned(), blob);
            Arc::new(Self(Mutex::new(map)))
        }

        fn get_stored_cred(&self) -> Option<StoredCredential> {
            let guard = self.0.lock().unwrap();
            let json = guard.get(TWITCH_CREDENTIAL_ID)?;
            let v: serde_json::Value = serde_json::from_str(json).ok()?;
            let access_token = OAuthToken::new(v["access_token"].as_str()?.to_owned());
            let refresh_token = v["refresh_token"]
                .as_str()
                .map(|s| OAuthToken::new(s.to_owned()));
            Some(StoredCredential {
                access_token,
                refresh_token,
                user_id: v["user_id"].as_str().unwrap_or("").to_owned(),
                login: v["login"].as_str().unwrap_or("").to_owned(),
                expires_at: None,
            })
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

    fn stub_cred(expires_at: SystemTime) -> StoredCredential {
        StoredCredential {
            access_token: OAuthToken::new("existing_access"),
            refresh_token: Some(OAuthToken::new("existing_refresh")),
            user_id: "user_1".to_owned(),
            login: "streamer".to_owned(),
            expires_at: Some(expires_at),
        }
    }

    fn manager_with_server(
        repo: Arc<dyn CredentialsRepo>,
        server: &MockServer,
    ) -> TwitchCredentialsManager {
        TwitchCredentialsManager::with_endpoint(
            repo,
            "test_client_id".to_owned(),
            format!("{}/token", server.uri()),
        )
    }

    // ---------------------------------------------------------------------------
    // get_valid_access_token: fresh credential - no network call.
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn get_valid_access_token_returns_stored_token_without_refresh_when_far_from_expiry() {
        let server = MockServer::start().await;
        // Expires 1 hour from now - well beyond the 5-minute buffer.
        let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(3600));
        let mgr = manager_with_server(InMemRepo::seeded(&cred), &server);

        let token = mgr.get_valid_access_token().await.unwrap();
        assert_eq!(token.expose(), "existing_access");

        // No request must have reached the mock server.
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "refresh endpoint must not be called for a fresh credential"
        );
    }

    // ---------------------------------------------------------------------------
    // get_valid_access_token: no credentials → ReauthRequired (no network).
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn get_valid_access_token_returns_reauth_required_when_no_credential_stored() {
        let server = MockServer::start().await;
        let mgr = manager_with_server(InMemRepo::empty(), &server);

        let err = mgr.get_valid_access_token().await.unwrap_err();
        assert!(
            matches!(err, PlatformError::ReauthRequired { .. }),
            "expected ReauthRequired when no credential exists, got: {err}"
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "no network call expected when credentials are absent"
        );
    }

    // ---------------------------------------------------------------------------
    // refresh: rotated refresh_token from upstream is persisted (RFC-091 §2).
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn refresh_persists_rotated_refresh_token_returned_by_upstream() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new_access",
                "refresh_token": "rotated_refresh",
                "expires_in": 14400,
            })))
            .mount(&server)
            .await;

        let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(60));
        let repo = InMemRepo::seeded(&cred);
        let mgr = manager_with_server(repo.clone(), &server);

        let prior_rt = OAuthToken::new("existing_refresh");
        mgr.refresh(&prior_rt).await.unwrap();

        let stored = repo.get_stored_cred().unwrap();
        assert_eq!(
            stored.refresh_token.as_ref().map(OAuthToken::expose),
            Some("rotated_refresh"),
            "rotated refresh_token from upstream must replace the prior one"
        );
        assert_eq!(stored.access_token.expose(), "new_access");
    }

    // ---------------------------------------------------------------------------
    // refresh: upstream omits refresh_token → prior one is retained (RFC-091 §2).
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn refresh_retains_prior_refresh_token_when_upstream_omits_it() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new_access_only",
                "expires_in": 14400,
            })))
            .mount(&server)
            .await;

        let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(60));
        let repo = InMemRepo::seeded(&cred);
        let mgr = manager_with_server(repo.clone(), &server);

        let prior_rt = OAuthToken::new("existing_refresh");
        mgr.refresh(&prior_rt).await.unwrap();

        let stored = repo.get_stored_cred().unwrap();
        assert_eq!(
            stored.refresh_token.as_ref().map(OAuthToken::expose),
            Some("existing_refresh"),
            "prior refresh_token must be retained when upstream omits a new one"
        );
    }

    // ---------------------------------------------------------------------------
    // refresh: HTTP 400 → ReauthRequired (RFC-091 §2).
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn refresh_returns_reauth_required_on_400() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"})),
            )
            .mount(&server)
            .await;

        let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(60));
        let mgr = manager_with_server(InMemRepo::seeded(&cred), &server);

        let err = mgr
            .refresh(&OAuthToken::new("expired_rt"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::ReauthRequired { .. }),
            "HTTP 400 must map to ReauthRequired, got: {err}"
        );
    }

    // ---------------------------------------------------------------------------
    // refresh: HTTP 401 → ReauthRequired (RFC-091 §2).
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn refresh_returns_reauth_required_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&server)
            .await;

        let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(60));
        let mgr = manager_with_server(InMemRepo::seeded(&cred), &server);

        let err = mgr
            .refresh(&OAuthToken::new("revoked_rt"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::ReauthRequired { .. }),
            "HTTP 401 must map to ReauthRequired, got: {err}"
        );
    }

    // ---------------------------------------------------------------------------
    // refresh: form body must not contain client_secret (public-client PKCE).
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn refresh_sends_form_without_client_secret() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("client_id=test_client_id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "na",
                "expires_in": 14400,
            })))
            .mount(&server)
            .await;

        let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(60));
        let repo = InMemRepo::seeded(&cred);
        let mgr = manager_with_server(repo, &server);
        mgr.refresh(&OAuthToken::new("existing_refresh"))
            .await
            .unwrap();

        let reqs = server.received_requests().await.unwrap();
        assert_eq!(reqs.len(), 1);
        let body = std::str::from_utf8(&reqs[0].body).unwrap();
        assert!(
            !body.contains("client_secret"),
            "client_secret must never appear in the token refresh form (public-client PKCE)"
        );
    }

    // ---------------------------------------------------------------------------
    // HelixTokenSource: no credentials → HelixError::ReauthRequired.
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn helix_token_source_returns_reauth_required_when_no_credential() {
        use crate::helix::HelixTokenSource;

        let server = MockServer::start().await;
        let mgr = manager_with_server(InMemRepo::empty(), &server);

        let err = mgr.access_token().await.unwrap_err();
        assert!(
            matches!(err, crate::helix::HelixError::ReauthRequired),
            "HelixTokenSource must map missing credential to HelixError::ReauthRequired, got: {err:?}"
        );
    }
}
