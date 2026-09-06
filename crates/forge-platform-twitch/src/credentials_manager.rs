use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_platform_core::auth::{
    PkceRefreshConfig, PkceRefresher, REFRESH_BUFFER_SECS, ReauthPolicy,
};
use forge_storage::{CredentialsRepo, StorageError};
use forge_types::OAuthToken;
use tracing::{debug, info, warn};

use crate::auth::TWITCH_TOKEN_ENDPOINT;
use crate::credentials::{StoredCredential, load, store_credential};

const PLATFORM: &str = "twitch";
const REFRESH_BUFFER: Duration = Duration::from_secs(REFRESH_BUFFER_SECS);

pub struct TwitchCredentialsManager {
    repo: Arc<dyn CredentialsRepo>,
    refresher: PkceRefresher,
    refresh_guard: tokio::sync::Mutex<()>,
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
        Self {
            repo,
            refresher,
            refresh_guard: tokio::sync::Mutex::new(()),
        }
    }

    pub async fn load(&self) -> Result<Option<StoredCredential>, PlatformError> {
        load(self.repo.as_ref()).await.map_err(storage_err)
    }

    /// Renews proactively within the refresh buffer; a refresh-token-less credential near expiry routes to re-auth.
    pub async fn get_valid_access_token(&self) -> Result<OAuthToken, PlatformError> {
        let cred = self.load().await?.ok_or_else(reauth_err)?;
        if !near_expiry(&cred) {
            return Ok(cred.access_token);
        }
        debug!(
            buffer_secs = REFRESH_BUFFER_SECS,
            "twitch access token inside the refresh buffer; renewing"
        );
        let _guard = self.refresh_guard.lock().await;
        let cred = self.load().await?.ok_or_else(reauth_err)?;
        if !near_expiry(&cred) {
            debug!("twitch token already renewed by a concurrent refresh");
            return Ok(cred.access_token);
        }
        let refresh_token = cred.refresh_token.clone().ok_or_else(reauth_err)?;
        let renewed = self.perform_refresh(&refresh_token, cred).await?;
        Ok(renewed.access_token)
    }

    /// Rechecks against the stored access token under the guard: a concurrent refresh may have
    /// already rotated the pair while this call waited, in which case Twitch is not called again.
    pub async fn refresh(
        &self,
        failed_access_token: &OAuthToken,
    ) -> Result<StoredCredential, PlatformError> {
        let _guard = self.refresh_guard.lock().await;
        let existing = self.load().await?.ok_or_else(reauth_err)?;
        if existing.access_token.expose() != failed_access_token.expose() {
            debug!("twitch token already rotated by a concurrent refresh; reusing it");
            return Ok(existing);
        }
        let refresh_token = existing.refresh_token.clone().ok_or_else(reauth_err)?;
        self.perform_refresh(&refresh_token, existing).await
    }

    /// Public-client refresh (no `client_secret`); keeps the prior refresh token only if the response omits a new one.
    async fn perform_refresh(
        &self,
        refresh_token: &OAuthToken,
        existing: StoredCredential,
    ) -> Result<StoredCredential, PlatformError> {
        let parsed = match self.refresher.refresh(refresh_token.expose()).await {
            Ok(parsed) => parsed,
            Err(e) => {
                // Display of `Http` carries the endpoint's response body; report the shape only.
                match &e {
                    PlatformError::ReauthRequired { .. } => {
                        warn!("twitch token refresh rejected; re-authorization required");
                    }
                    PlatformError::Http { status, .. } => {
                        warn!(status = *status, "twitch token refresh failed");
                    }
                    PlatformError::Network { reason } => {
                        warn!(error = %reason, "twitch token refresh failed");
                    }
                    _ => warn!("twitch token refresh failed"),
                }
                return Err(e);
            }
        };
        info!(
            expires_in_secs = ?parsed.expires_in,
            rotated_refresh_token = parsed.refresh_token.is_some(),
            "twitch access token refreshed"
        );

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

fn near_expiry(cred: &StoredCredential) -> bool {
    cred.expires_at
        .map(|at| at <= SystemTime::now() + REFRESH_BUFFER)
        .unwrap_or(false)
}

#[async_trait]
impl crate::helix::HelixTokenSource for TwitchCredentialsManager {
    async fn access_token(&self) -> Result<OAuthToken, crate::helix::HelixError> {
        self.get_valid_access_token().await.map_err(|e| match e {
            PlatformError::ReauthRequired { .. } => crate::helix::HelixError::ReauthRequired,
            PlatformError::Io(io) => crate::helix::HelixError::Credentials(io.to_string()),
            // Display of `Http` carries the token endpoint's response body, and this string reaches sub-action error text and run history.
            PlatformError::Http { status, .. } => crate::helix::HelixError::Credentials(format!(
                "twitch token refresh failed: HTTP {status}"
            )),
            PlatformError::Network { reason } => crate::helix::HelixError::Credentials(format!(
                "twitch token refresh failed: {reason}"
            )),
            _ => crate::helix::HelixError::Credentials("twitch token refresh failed".to_owned()),
        })
    }
}

#[async_trait]
impl crate::helix::HelixTokenRefresher for TwitchCredentialsManager {
    async fn refresh(
        &self,
        failed_token: &OAuthToken,
    ) -> Result<OAuthToken, crate::helix::HelixError> {
        match self.refresh(failed_token).await {
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

    struct InMemRepo(Mutex<HashMap<String, String>>);

    impl InMemRepo {
        fn empty() -> Arc<Self> {
            Arc::new(Self(Mutex::new(HashMap::new())))
        }

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

    #[tokio::test]
    async fn get_valid_access_token_returns_stored_token_without_refresh_when_far_from_expiry() {
        let server = MockServer::start().await;
        let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(3600));
        let mgr = manager_with_server(InMemRepo::seeded(&cred), &server);

        let token = mgr.get_valid_access_token().await.unwrap();
        assert_eq!(token.expose(), "existing_access");

        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "refresh endpoint must not be called for a fresh credential"
        );
    }

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

        let stale_access = OAuthToken::new("existing_access");
        mgr.refresh(&stale_access).await.unwrap();

        let stored = repo.get_stored_cred().unwrap();
        assert_eq!(
            stored.refresh_token.as_ref().map(OAuthToken::expose),
            Some("rotated_refresh"),
            "rotated refresh_token from upstream must replace the prior one"
        );
        assert_eq!(stored.access_token.expose(), "new_access");
    }

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

        let stale_access = OAuthToken::new("existing_access");
        mgr.refresh(&stale_access).await.unwrap();

        let stored = repo.get_stored_cred().unwrap();
        assert_eq!(
            stored.refresh_token.as_ref().map(OAuthToken::expose),
            Some("existing_refresh"),
            "prior refresh_token must be retained when upstream omits a new one"
        );
    }

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
            .refresh(&OAuthToken::new("existing_access"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::ReauthRequired { .. }),
            "HTTP 400 must map to ReauthRequired, got: {err}"
        );
    }

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
            .refresh(&OAuthToken::new("existing_access"))
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::ReauthRequired { .. }),
            "HTTP 401 must map to ReauthRequired, got: {err}"
        );
    }

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
        mgr.refresh(&OAuthToken::new("existing_access"))
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

    const BODY_SENTINEL: &str = "REFRESH_BODY_SENTINEL_p3k";

    /// Drives one failing refresh against a mock token endpoint whose body carries the sentinel,
    /// capturing everything from TRACE up so no tier can hide a leak.
    fn failing_refresh(status: u16) -> (PlatformError, Vec<crate::log_capture::CapturedLine>) {
        crate::log_capture::capture_blocking(tracing::Level::TRACE, async move {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(status).set_body_json(json!({
                    "error": "invalid_grant",
                    "error_description": BODY_SENTINEL,
                })))
                .mount(&server)
                .await;

            let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(60));
            let mgr = manager_with_server(InMemRepo::seeded(&cred), &server);
            mgr.refresh(&OAuthToken::new("existing_access"))
                .await
                .unwrap_err()
        })
    }

    #[test]
    fn token_refresh_failure_never_logs_the_endpoint_response_body() {
        for status in [400, 503] {
            let (err, lines) = failing_refresh(status);
            if let PlatformError::Http { body, .. } = &err {
                assert!(
                    body.contains(BODY_SENTINEL),
                    "fixture must reproduce the leak vector: the error itself carries the body"
                );
            }

            let forge_lines = crate::log_capture::forge_lines(&lines);
            assert!(
                !forge_lines.is_empty(),
                "HTTP {status}: the refresh path must have logged something to inspect"
            );
            for line in forge_lines {
                assert!(
                    !line.mentions(BODY_SENTINEL),
                    "HTTP {status}: token-endpoint response body reached a {} line: {:?}",
                    line.level,
                    line.fields
                );
            }
        }
    }

    type WarnExpectation = fn(&crate::log_capture::CapturedLine) -> bool;

    #[test]
    fn token_refresh_failure_warns_with_the_shape_of_its_branch() {
        // Why: the two branches must stay distinguishable to an operator - a reauth needs the
        // sign-in banner, an upstream 5xx needs a retry - and neither may widen past the shape.
        let cases: Vec<(u16, WarnExpectation)> = vec![
            (400, |line| {
                line.message().contains("re-authorization required")
                    && line.field("status").is_none()
            }),
            (503, |line| line.field("status") == Some("503")),
        ];

        for (status, is_expected) in cases {
            let (_, lines) = failing_refresh(status);
            let warns: Vec<_> = crate::log_capture::forge_lines(&lines)
                .into_iter()
                .filter(|line| line.level == tracing::Level::WARN)
                .collect();

            assert!(
                warns.iter().any(|line| is_expected(line)),
                "HTTP {status}: no WARN line reported the expected branch shape; got {:?}",
                warns.iter().map(|l| &l.fields).collect::<Vec<_>>()
            );
        }
    }

    #[tokio::test]
    async fn token_source_error_reports_the_refresh_shape_without_the_endpoint_body() {
        // Why: this string is not only logged - it reaches sub-action failure text and run
        // history, so `PlatformError`'s `HTTP {status}: {body}` Display must not be forwarded.
        use crate::helix::HelixTokenSource;

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(503).set_body_json(json!({
                "message": BODY_SENTINEL,
            })))
            .mount(&server)
            .await;
        let cred = stub_cred(SystemTime::now() + std::time::Duration::from_secs(60));
        let mgr = manager_with_server(InMemRepo::seeded(&cred), &server);

        let rendered = mgr.access_token().await.unwrap_err().to_string();

        assert!(
            !rendered.contains(BODY_SENTINEL),
            "token-endpoint response body reached the HelixError string: {rendered}"
        );
        assert!(
            rendered.contains("HTTP 503"),
            "the upstream status must survive so an operator can tell a 5xx from a network drop; \
             got: {rendered}"
        );
    }

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
