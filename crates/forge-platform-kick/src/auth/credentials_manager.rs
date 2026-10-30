use std::sync::Arc;

use forge_net::Gateway;
use forge_platform_core::auth::REFRESH_BUFFER_SECS;
use forge_platform_core::{PlatformError, credential_storage_error, reauth_required};
use forge_storage::{CredentialId, CredentialsRepo};
use time::{Duration, OffsetDateTime};
use tokio::sync::Mutex;

use crate::auth::credentials::{CREDENTIAL_KEY, KickCredentials};
use crate::auth::oauth_client::KickOAuthClient;
use crate::auth::{KickAuthBundle, expiry_after};

const PLATFORM: &str = "kick";
const REFRESH_BUFFER: Duration = Duration::seconds(REFRESH_BUFFER_SECS as i64);
const REFRESH_TASK_LOST: &str = "token refresh did not complete";

pub struct KickCredentialsManager {
    store: CredentialStore,
    oauth: KickOAuthClient,
    api: Gateway,
    refresh_guard: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct CredentialStore {
    repo: Arc<dyn CredentialsRepo>,
}

enum RefreshTrigger {
    NearExpiry,
    Rejected(String),
}

impl KickCredentialsManager {
    pub fn new(repo: Arc<dyn CredentialsRepo>, oauth: KickOAuthClient, api: Gateway) -> Self {
        Self {
            store: CredentialStore { repo },
            oauth,
            api,
            refresh_guard: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn oauth_client(&self) -> &KickOAuthClient {
        &self.oauth
    }

    pub(crate) fn api(&self) -> &Gateway {
        &self.api
    }

    pub async fn load(&self) -> Result<Option<KickCredentials>, PlatformError> {
        self.store.load().await
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
        let _guard = self.refresh_guard.lock().await;
        self.store.persist(&creds).await
    }

    pub async fn get_valid_access_token(&self) -> Result<String, PlatformError> {
        let creds = self.store.require().await?;
        if !near_expiry(&creds) {
            return Ok(creds.access_token.expose().to_owned());
        }
        self.detached_refresh(RefreshTrigger::NearExpiry)
            .await
            .map(|creds| creds.access_token.expose().to_owned())
    }

    pub async fn user_id(&self) -> Result<u64, PlatformError> {
        Ok(self.store.require().await?.user_id)
    }

    pub async fn refresh(
        &self,
        failed_access_token: &str,
    ) -> Result<KickCredentials, PlatformError> {
        self.detached_refresh(RefreshTrigger::Rejected(failed_access_token.to_owned()))
            .await
    }

    async fn detached_refresh(
        &self,
        trigger: RefreshTrigger,
    ) -> Result<KickCredentials, PlatformError> {
        let store = self.store.clone();
        let oauth = self.oauth.clone();
        let guard = Arc::clone(&self.refresh_guard);
        tokio::spawn(async move {
            let _guard = guard.lock().await;
            refresh_under_guard(&store, &oauth, trigger).await
        })
        .await
        .map_err(|_| PlatformError::Auth {
            reason: REFRESH_TASK_LOST.to_owned(),
        })?
    }

    pub async fn clear(&self) -> Result<(), PlatformError> {
        let _guard = self.refresh_guard.lock().await;
        self.store.clear().await
    }
}

async fn refresh_under_guard(
    store: &CredentialStore,
    oauth: &KickOAuthClient,
    trigger: RefreshTrigger,
) -> Result<KickCredentials, PlatformError> {
    let existing = store.require().await?;
    let already_rotated = match &trigger {
        RefreshTrigger::NearExpiry => !near_expiry(&existing),
        RefreshTrigger::Rejected(failed) => existing.access_token.expose() != failed,
    };
    if already_rotated {
        return Ok(existing);
    }

    let grant = oauth.refresh(existing.refresh_token.expose()).await?;
    let updated = KickCredentials {
        access_token: grant.access_token,
        refresh_token: grant.refresh_token.unwrap_or(existing.refresh_token),
        user_id: existing.user_id,
        username: existing.username,
        client_id: existing.client_id,
        expires_at: expiry_after(grant.expires_in)?,
    };
    store.persist(&updated).await?;
    Ok(updated)
}

impl CredentialStore {
    async fn load(&self) -> Result<Option<KickCredentials>, PlatformError> {
        let Some(json) = self
            .repo
            .load(&CredentialId::new(CREDENTIAL_KEY))
            .await
            .map_err(|e| credential_storage_error(PLATFORM, e.stored_secret_unrecoverable(), e))?
        else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&json)?))
    }

    async fn require(&self) -> Result<KickCredentials, PlatformError> {
        self.load().await?.ok_or_else(|| reauth_required(PLATFORM))
    }

    async fn persist(&self, creds: &KickCredentials) -> Result<(), PlatformError> {
        let json = serde_json::to_string(creds)?;
        self.repo
            .store(&CredentialId::new(CREDENTIAL_KEY), &json)
            .await
            .map_err(|e| credential_storage_error(PLATFORM, e.stored_secret_unrecoverable(), e))
    }

    async fn clear(&self) -> Result<(), PlatformError> {
        self.repo
            .delete(&CredentialId::new(CREDENTIAL_KEY))
            .await
            .map_err(|e| credential_storage_error(PLATFORM, e.stored_secret_unrecoverable(), e))?;
        Ok(())
    }
}

fn near_expiry(creds: &KickCredentials) -> bool {
    creds.expires_at <= OffsetDateTime::now_utc() + REFRESH_BUFFER
}

#[cfg(test)]
mod tests {
    use forge_types::OAuthToken;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use forge_platform_core::PlatformError;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use serde_json::json;
    use time::{Duration, OffsetDateTime};
    use tokio::sync::Notify;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::KickCredentialsManager;
    use crate::auth::KickAuthBundle;
    use crate::auth::credentials::{CREDENTIAL_KEY, KickCredentials};
    use crate::endpoints_test_support::{oauth_client_at, offline_manager, unreachable_api};

    const TOKEN_PATH: &str = "/token";
    const EXISTING_ACCESS: &str = "kick_access_existing";
    const EXISTING_REFRESH: &str = "kick_refresh_existing";
    const REFRESHED_ACCESS: &str = "kick_access_refreshed";
    const ROTATED_REFRESH: &str = "kick_refresh_rotated";
    const SECOND_ACCESS: &str = "kick_access_second_refresh";
    const BUNDLE_ACCESS: &str = "kick_access_from_new_sign_in";
    const CONCURRENT_CALLERS: usize = 4;
    const SETTLE_TURNS: usize = 256;

    #[derive(Default)]
    struct FirstStoreGate {
        armed: AtomicBool,
        arrived: Notify,
        release: Notify,
    }

    impl FirstStoreGate {
        fn armed() -> Arc<Self> {
            let gate = Self::default();
            gate.armed.store(true, Ordering::SeqCst);
            Arc::new(gate)
        }

        async fn arrival(&self) {
            self.arrived.notified().await;
        }

        fn open(&self) {
            self.release.notify_one();
        }

        async fn pass(&self) {
            if self.armed.swap(false, Ordering::SeqCst) {
                self.arrived.notify_one();
                self.release.notified().await;
            }
        }
    }

    #[derive(Default)]
    struct InMemRepo {
        rows: Mutex<HashMap<String, String>>,
        gate: Option<Arc<FirstStoreGate>>,
    }

    impl InMemRepo {
        fn holding(creds: &KickCredentials, gate: Option<Arc<FirstStoreGate>>) -> Arc<Self> {
            let repo = Self {
                rows: Mutex::default(),
                gate,
            };
            repo.rows.lock().unwrap().insert(
                CREDENTIAL_KEY.to_owned(),
                serde_json::to_string(creds).unwrap(),
            );
            Arc::new(repo)
        }

        fn stored(&self) -> Option<KickCredentials> {
            self.rows
                .lock()
                .unwrap()
                .get(CREDENTIAL_KEY)
                .and_then(|s| serde_json::from_str(s).ok())
        }
    }

    #[async_trait]
    impl CredentialsRepo for InMemRepo {
        async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
            if let Some(gate) = &self.gate {
                gate.pass().await;
            }
            self.rows
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), plaintext.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.rows.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.rows.lock().unwrap().remove(id.as_str()).is_some())
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(self
                .rows
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

    struct PanickingRepo;

    #[async_trait]
    impl CredentialsRepo for PanickingRepo {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }

        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            panic!("credential store crashed")
        }

        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(Vec::new())
        }

        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    struct LoadFailsRepo(fn() -> StorageError);

    #[async_trait]
    impl CredentialsRepo for LoadFailsRepo {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }

        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Err((self.0)())
        }

        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(Vec::new())
        }

        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }

        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn stub_creds(expires_at: OffsetDateTime) -> KickCredentials {
        KickCredentials {
            access_token: OAuthToken::new(EXISTING_ACCESS),
            refresh_token: OAuthToken::new(EXISTING_REFRESH),
            user_id: 42,
            username: "streamer".to_owned(),
            client_id: "test_cid".to_owned(),
            expires_at,
        }
    }

    fn fresh() -> KickCredentials {
        stub_creds(OffsetDateTime::now_utc() + Duration::hours(1))
    }

    fn near_expiry() -> KickCredentials {
        stub_creds(OffsetDateTime::now_utc() + Duration::seconds(60))
    }

    fn refreshed_grant(access: &str) -> ResponseTemplate {
        ResponseTemplate::new(reqwest::StatusCode::OK.as_u16()).set_body_json(json!({
            "access_token": access,
            "refresh_token": ROTATED_REFRESH,
            "expires_in": 86400
        }))
    }

    fn bundle() -> KickAuthBundle {
        KickAuthBundle {
            access_token: OAuthToken::new(BUNDLE_ACCESS),
            refresh_token: OAuthToken::new("kick_refresh_from_new_sign_in"),
            user_id: 99,
            username: "bundle_streamer".to_owned(),
            client_id: "bundle_cid".to_owned(),
            expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
        }
    }

    struct Rig {
        server: MockServer,
        repo: Arc<InMemRepo>,
        manager: Arc<KickCredentialsManager>,
    }

    impl Rig {
        async fn token_posts(&self) -> usize {
            self.server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|request| request.url.path() == TOKEN_PATH)
                .count()
        }

        async fn answer_once_then_always(&self, first: ResponseTemplate, rest: ResponseTemplate) {
            Mock::given(method("POST"))
                .and(path(TOKEN_PATH))
                .respond_with(first)
                .up_to_n_times(1)
                .with_priority(1)
                .mount(&self.server)
                .await;
            self.answer_always(rest).await;
        }

        async fn answer_always(&self, reply: ResponseTemplate) {
            Mock::given(method("POST"))
                .and(path(TOKEN_PATH))
                .respond_with(reply)
                .mount(&self.server)
                .await;
        }
    }

    async fn rig_gated(creds: &KickCredentials, gate: Option<Arc<FirstStoreGate>>) -> Rig {
        let server = MockServer::start().await;
        let repo = InMemRepo::holding(creds, gate);
        let manager = Arc::new(KickCredentialsManager::new(
            repo.clone(),
            oauth_client_at(&server.uri()),
            unreachable_api(),
        ));
        Rig {
            server,
            repo,
            manager,
        }
    }

    async fn rig(creds: &KickCredentials) -> Rig {
        rig_gated(creds, None).await
    }

    async fn settle() {
        for _ in 0..SETTLE_TURNS {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn load_returns_none_when_no_row() {
        let manager = offline_manager(Arc::new(InMemRepo::default()));

        assert!(manager.load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn load_asks_for_reauth_only_when_the_stored_secret_cannot_be_decrypted() {
        let undecryptable = offline_manager(Arc::new(LoadFailsRepo(|| StorageError::Decryption)));
        let locked = offline_manager(Arc::new(LoadFailsRepo(|| StorageError::Connection {
            reason: "database is locked".to_owned(),
        })));

        let undecryptable = undecryptable.load().await.unwrap_err();
        let locked = locked.load().await.unwrap_err();

        assert!(
            matches!(&undecryptable, PlatformError::ReauthRequired { platform } if platform == "kick"),
            "undecryptable row: got {undecryptable:?}"
        );
        assert!(
            matches!(&locked, PlatformError::Io(_)),
            "locked store must stay retryable: got {locked:?}"
        );
    }

    #[tokio::test]
    async fn save_from_bundle_persists_credentials() {
        let repo = Arc::new(InMemRepo::default());
        let manager = offline_manager(repo.clone());

        manager.save_from_bundle(bundle()).await.unwrap();

        let stored = repo.stored().unwrap();
        assert_eq!(
            (
                stored.access_token.expose(),
                stored.refresh_token.expose(),
                stored.user_id,
                stored.username.as_str(),
                stored.client_id.as_str()
            ),
            (
                BUNDLE_ACCESS,
                "kick_refresh_from_new_sign_in",
                99,
                "bundle_streamer",
                "bundle_cid"
            )
        );
    }

    #[tokio::test]
    async fn get_valid_access_token_returns_existing_when_fresh() {
        let rig = rig(&fresh()).await;

        let token = rig.manager.get_valid_access_token().await.unwrap();

        assert_eq!(
            (token.as_str(), rig.token_posts().await),
            (EXISTING_ACCESS, 0)
        );
    }

    #[tokio::test]
    async fn get_valid_access_token_refreshes_and_persists_a_token_near_expiry() {
        let rig = rig(&near_expiry()).await;
        rig.answer_always(refreshed_grant(REFRESHED_ACCESS)).await;

        let token = rig.manager.get_valid_access_token().await.unwrap();

        let stored = rig.repo.stored().unwrap();
        assert_eq!(
            (
                token.as_str(),
                stored.access_token.expose(),
                stored.refresh_token.expose(),
                stored.user_id,
                stored.expires_at > OffsetDateTime::now_utc() + Duration::hours(23)
            ),
            (
                REFRESHED_ACCESS,
                REFRESHED_ACCESS,
                ROTATED_REFRESH,
                42,
                true
            )
        );
    }

    #[tokio::test]
    async fn get_valid_access_token_returns_reauth_required_when_no_creds() {
        let manager = offline_manager(Arc::new(InMemRepo::default()));

        let err = manager.get_valid_access_token().await.unwrap_err();

        assert!(
            matches!(err, PlatformError::ReauthRequired { .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn refresh_keeps_the_stored_refresh_token_when_kick_omits_it() {
        let rig = rig(&fresh()).await;
        rig.answer_always(
            ResponseTemplate::new(reqwest::StatusCode::OK.as_u16())
                .set_body_json(json!({ "access_token": "kick_access_only", "expires_in": 86400 })),
        )
        .await;

        rig.manager.refresh(EXISTING_ACCESS).await.unwrap();

        let stored = rig.repo.stored().unwrap();
        assert_eq!(
            (stored.access_token.expose(), stored.refresh_token.expose()),
            ("kick_access_only", EXISTING_REFRESH)
        );
    }

    #[tokio::test]
    async fn refresh_of_an_already_replaced_token_returns_the_stored_one_without_a_request() {
        let rig = rig(&fresh()).await;

        let creds = rig.manager.refresh("kick_access_long_gone").await.unwrap();

        assert_eq!(
            (creds.access_token.expose(), rig.token_posts().await),
            (EXISTING_ACCESS, 0)
        );
    }

    #[tokio::test]
    async fn a_failed_or_unusable_refresh_leaves_the_stored_credentials_untouched() {
        for (label, reply, reauth) in [
            (
                "400 refused grant",
                ResponseTemplate::new(reqwest::StatusCode::BAD_REQUEST.as_u16())
                    .set_body_json(json!({ "error": "Invalid request" })),
                true,
            ),
            (
                "lifetime past the calendar",
                ResponseTemplate::new(reqwest::StatusCode::OK.as_u16()).set_body_json(json!({
                    "access_token": REFRESHED_ACCESS,
                    "refresh_token": ROTATED_REFRESH,
                    "expires_in": u64::MAX
                })),
                false,
            ),
        ] {
            let rig = rig(&near_expiry()).await;
            rig.answer_always(reply).await;

            let outcome = rig.manager.get_valid_access_token().await;

            let stored = rig.repo.stored().unwrap();
            let mapped = match outcome {
                Err(PlatformError::ReauthRequired { .. }) => reauth,
                Err(PlatformError::MalformedResponse { .. }) => !reauth,
                _ => false,
            };
            assert_eq!(
                (
                    mapped,
                    stored.access_token.expose(),
                    stored.refresh_token.expose()
                ),
                (true, EXISTING_ACCESS, EXISTING_REFRESH),
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn concurrent_token_requests_and_refreshes_share_one_token_post() {
        let gate = FirstStoreGate::armed();
        let rig = rig_gated(&near_expiry(), Some(Arc::clone(&gate))).await;
        rig.answer_once_then_always(
            refreshed_grant(REFRESHED_ACCESS),
            refreshed_grant(SECOND_ACCESS),
        )
        .await;
        let mut callers = Vec::new();
        for caller in 0..CONCURRENT_CALLERS {
            let manager = Arc::clone(&rig.manager);
            callers.push(tokio::spawn(async move {
                if caller % 2 == 0 {
                    manager.get_valid_access_token().await
                } else {
                    manager
                        .refresh(EXISTING_ACCESS)
                        .await
                        .map(|c| c.access_token.expose().to_owned())
                }
            }));
        }
        gate.arrival().await;
        settle().await;
        gate.open();

        let mut tokens = Vec::new();
        for caller in callers {
            tokens.push(caller.await.unwrap().unwrap());
        }

        assert_eq!(
            (tokens, rig.token_posts().await),
            (vec![REFRESHED_ACCESS.to_owned(); CONCURRENT_CALLERS], 1)
        );
    }

    #[tokio::test]
    async fn a_cancelled_waiter_leaves_the_refresh_running_for_the_other_waiters() {
        let gate = FirstStoreGate::armed();
        let rig = rig_gated(&near_expiry(), Some(Arc::clone(&gate))).await;
        rig.answer_once_then_always(
            refreshed_grant(REFRESHED_ACCESS),
            refreshed_grant(SECOND_ACCESS),
        )
        .await;
        let cancelled = tokio::spawn({
            let manager = Arc::clone(&rig.manager);
            async move { manager.get_valid_access_token().await }
        });
        gate.arrival().await;
        cancelled.abort();
        let aborted = cancelled.await.unwrap_err().is_cancelled();
        let survivor = tokio::spawn({
            let manager = Arc::clone(&rig.manager);
            async move { manager.get_valid_access_token().await }
        });
        settle().await;
        gate.open();

        let token = survivor.await.unwrap().unwrap();

        let stored = rig.repo.stored().unwrap();
        assert_eq!(
            (
                aborted,
                token.as_str(),
                stored.access_token.expose(),
                stored.refresh_token.expose(),
                rig.token_posts().await
            ),
            (true, REFRESHED_ACCESS, REFRESHED_ACCESS, ROTATED_REFRESH, 1)
        );
    }

    #[tokio::test]
    async fn clear_during_a_refresh_waits_so_logout_is_never_undone() {
        let gate = FirstStoreGate::armed();
        let rig = rig_gated(&near_expiry(), Some(Arc::clone(&gate))).await;
        rig.answer_always(refreshed_grant(REFRESHED_ACCESS)).await;
        let refreshing = tokio::spawn({
            let manager = Arc::clone(&rig.manager);
            async move { manager.get_valid_access_token().await }
        });
        gate.arrival().await;
        let clearing = tokio::spawn({
            let manager = Arc::clone(&rig.manager);
            async move { manager.clear().await }
        });
        settle().await;
        let row_while_refresh_in_flight = rig.repo.stored().is_some();
        gate.open();

        refreshing.await.unwrap().unwrap();
        clearing.await.unwrap().unwrap();

        assert_eq!(
            (row_while_refresh_in_flight, rig.repo.stored().is_none()),
            (true, true)
        );
    }

    #[tokio::test]
    async fn a_sign_in_saved_during_a_refresh_is_never_overwritten_by_it() {
        let gate = FirstStoreGate::armed();
        let rig = rig_gated(&near_expiry(), Some(Arc::clone(&gate))).await;
        rig.answer_always(refreshed_grant(REFRESHED_ACCESS)).await;
        let refreshing = tokio::spawn({
            let manager = Arc::clone(&rig.manager);
            async move { manager.get_valid_access_token().await }
        });
        gate.arrival().await;
        let saving = tokio::spawn({
            let manager = Arc::clone(&rig.manager);
            async move { manager.save_from_bundle(bundle()).await }
        });
        settle().await;
        gate.open();

        refreshing.await.unwrap().unwrap();
        saving.await.unwrap().unwrap();

        assert_eq!(
            rig.repo.stored().unwrap().access_token.expose(),
            BUNDLE_ACCESS
        );
    }

    #[tokio::test]
    async fn a_refresh_task_that_dies_reports_an_auth_error() {
        let manager = offline_manager(Arc::new(PanickingRepo));

        let outcome = manager.refresh(EXISTING_ACCESS).await;

        assert!(
            matches!(outcome, Err(PlatformError::Auth { .. })),
            "{:?}",
            outcome.err()
        );
    }

    #[tokio::test]
    async fn clear_removes_credentials_row() {
        let rig = rig(&fresh()).await;

        rig.manager.clear().await.unwrap();

        assert!(rig.repo.stored().is_none());
    }
}
