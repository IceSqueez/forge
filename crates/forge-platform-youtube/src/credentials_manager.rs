use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use forge_platform_core::auth::REFRESH_BUFFER_SECS;
use forge_platform_core::{PlatformError, credential_storage_error, reauth_required};
use forge_storage::{CredentialId, CredentialsRepo};
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex, watch};

use crate::auth::{YoutubeAuthBundle, fetch_own_channel};
use crate::credentials::{
    CREDENTIAL_KEY, DEFAULT_ACCESS_TOKEN_LIFETIME_SECS, YoutubeBroadcaster, YoutubeCredentials,
};
use crate::data_api::YoutubeDataApi;
use crate::oauth_client::GoogleOAuthClient;

const PLATFORM: &str = "youtube";
const REFRESH_BUFFER: Duration = Duration::seconds(REFRESH_BUFFER_SECS as i64);
const REFRESH_TASK_LOST: &str = "token refresh did not complete";

pub struct YoutubeCredentialsManager {
    store: CredentialStore,
    oauth: GoogleOAuthClient,
    api: YoutubeDataApi,
    refresh_guard: Arc<Mutex<()>>,
}

#[derive(Clone)]
struct CredentialStore {
    repo: Arc<dyn CredentialsRepo>,
    expiry_tx: Arc<watch::Sender<Option<SystemTime>>>,
}

enum RefreshTrigger {
    NearExpiry,
    Rejected(String),
}

impl YoutubeCredentialsManager {
    pub fn new(
        repo: Arc<dyn CredentialsRepo>,
        oauth: GoogleOAuthClient,
        api: YoutubeDataApi,
    ) -> Self {
        Self {
            store: CredentialStore {
                repo,
                expiry_tx: Arc::new(watch::channel(None).0),
            },
            oauth,
            api,
            refresh_guard: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn subscribe_expiry(&self) -> watch::Receiver<Option<SystemTime>> {
        self.store.expiry_tx.subscribe()
    }

    pub(crate) fn oauth_client(&self) -> &GoogleOAuthClient {
        &self.oauth
    }

    pub(crate) fn data_api(&self) -> &YoutubeDataApi {
        &self.api
    }

    pub async fn load(&self) -> Result<Option<YoutubeCredentials>, PlatformError> {
        self.store.load().await
    }

    pub async fn broadcaster(&self) -> Result<YoutubeBroadcaster, PlatformError> {
        let creds = self.store.require().await?;
        Ok(YoutubeBroadcaster {
            channel_id: creds.channel_id,
            channel_title: creds.channel_title,
        })
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
            access_token: bundle.access_token,
            refresh_token: bundle.refresh_token,
            client_id: bundle.client_id,
            channel_id: bundle.channel_id,
            channel_title: bundle.channel_title,
            channel_handle: bundle.channel_handle,
            expires_at,
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

    pub async fn refresh(
        &self,
        failed_access_token: &str,
    ) -> Result<YoutubeCredentials, PlatformError> {
        self.detached_refresh(RefreshTrigger::Rejected(failed_access_token.to_owned()))
            .await
    }

    async fn detached_refresh(
        &self,
        trigger: RefreshTrigger,
    ) -> Result<YoutubeCredentials, PlatformError> {
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

    pub async fn ensure_channel_handle(&self) -> Result<YoutubeCredentials, PlatformError> {
        let creds = self.store.require().await?;
        if creds.channel_handle.is_some() {
            return Ok(creds);
        }

        let Ok(token) = self.get_valid_access_token().await else {
            return Ok(creds);
        };
        let Ok(Some(channel)) = fetch_own_channel(&self.api, &token).await else {
            return Ok(creds);
        };
        let Some(handle) = channel.snippet.custom_url else {
            return Ok(creds);
        };

        let _guard = self.refresh_guard.lock().await;
        let mut latest = self.store.require().await?;
        latest.channel_handle = Some(handle);
        self.store.persist(&latest).await?;
        Ok(latest)
    }

    pub async fn clear(&self) -> Result<(), PlatformError> {
        let _guard = self.refresh_guard.lock().await;
        self.store.clear().await
    }
}

async fn refresh_under_guard(
    store: &CredentialStore,
    oauth: &GoogleOAuthClient,
    trigger: RefreshTrigger,
) -> Result<YoutubeCredentials, PlatformError> {
    let existing = store.require().await?;
    let already_rotated = match &trigger {
        RefreshTrigger::NearExpiry => !near_expiry(&existing),
        RefreshTrigger::Rejected(failed) => existing.access_token.expose() != failed,
    };
    if already_rotated {
        return Ok(existing);
    }

    let grant = oauth.refresh(existing.refresh_token.expose()).await?;
    let expires_in = i64::try_from(
        grant
            .expires_in
            .unwrap_or(DEFAULT_ACCESS_TOKEN_LIFETIME_SECS),
    )
    .unwrap_or(i64::MAX);
    let expires_at = OffsetDateTime::now_utc()
        .checked_add(Duration::seconds(expires_in))
        .ok_or_else(|| PlatformError::MalformedResponse {
            reason: "token lifetime out of range".to_owned(),
        })?;
    let updated = YoutubeCredentials {
        access_token: grant.access_token,
        refresh_token: grant.refresh_token.unwrap_or(existing.refresh_token),
        client_id: existing.client_id,
        channel_id: existing.channel_id,
        channel_title: existing.channel_title,
        channel_handle: existing.channel_handle,
        expires_at,
    };
    store.persist(&updated).await?;
    Ok(updated)
}

impl CredentialStore {
    async fn load(&self) -> Result<Option<YoutubeCredentials>, PlatformError> {
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

    async fn require(&self) -> Result<YoutubeCredentials, PlatformError> {
        self.load().await?.ok_or_else(|| reauth_required(PLATFORM))
    }

    async fn persist(&self, creds: &YoutubeCredentials) -> Result<(), PlatformError> {
        let json = serde_json::to_string(creds)?;
        self.repo
            .store(&CredentialId::new(CREDENTIAL_KEY), &json)
            .await
            .map_err(|e| credential_storage_error(PLATFORM, e.stored_secret_unrecoverable(), e))?;
        self.expiry_tx
            .send_replace(Some(SystemTime::from(creds.expires_at)));
        Ok(())
    }

    async fn clear(&self) -> Result<(), PlatformError> {
        self.repo
            .delete(&CredentialId::new(CREDENTIAL_KEY))
            .await
            .map_err(|e| credential_storage_error(PLATFORM, e.stored_secret_unrecoverable(), e))?;
        self.expiry_tx.send_replace(None);
        Ok(())
    }
}

fn near_expiry(creds: &YoutubeCredentials) -> bool {
    creds.expires_at <= OffsetDateTime::now_utc() + REFRESH_BUFFER
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration as StdDuration, SystemTime};

    use async_trait::async_trait;
    use forge_platform_core::PlatformError;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use forge_types::OAuthToken;
    use serde_json::json;
    use time::{Duration, OffsetDateTime};

    use super::YoutubeCredentialsManager;
    use crate::auth::YoutubeAuthBundle;
    use crate::credentials::{
        CREDENTIAL_KEY, DEFAULT_ACCESS_TOKEN_LIFETIME_SECS, YoutubeCredentials,
    };
    use crate::endpoints_test_support::{
        RecordingPublisher, ReplyGate, StubReply, StubServer, data_api_with, oauth_client_at,
        settle, unreachable_data_api, unreachable_oauth_client, used_today,
    };
    use crate::quota_state::SharedQuota;

    const TOKEN_PATH: &str = "/token";
    const CHANNELS_PATH: &str = "/channels";
    const EXISTING_ACCESS: &str = "ya29.existing";
    const EXISTING_REFRESH: &str = "1//existing_refresh";
    const REFRESHED_ACCESS: &str = "ya29.refreshed";
    const ROTATED_REFRESH: &str = "1//rotated_refresh";
    const SECOND_ACCESS: &str = "ya29.second_refresh";
    const REQUEST_FAIL_KIND: &str = "request.fail";
    const CONCURRENT_CALLERS: usize = 4;

    #[derive(Default)]
    struct InMemRepo(Mutex<HashMap<String, String>>);

    impl InMemRepo {
        fn with_creds(creds: &YoutubeCredentials) -> Arc<Self> {
            let repo = Self::default();
            repo.0.lock().unwrap().insert(
                CREDENTIAL_KEY.to_owned(),
                serde_json::to_string(creds).unwrap(),
            );
            Arc::new(repo)
        }

        fn stored(&self) -> Option<YoutubeCredentials> {
            self.0
                .lock()
                .unwrap()
                .get(CREDENTIAL_KEY)
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

    fn stub_creds(expires_at: OffsetDateTime) -> YoutubeCredentials {
        YoutubeCredentials {
            access_token: OAuthToken::new(EXISTING_ACCESS),
            refresh_token: OAuthToken::new(EXISTING_REFRESH),
            client_id: "test_client_id".to_owned(),
            channel_id: "UCtest123".to_owned(),
            channel_title: "Test Channel".to_owned(),
            channel_handle: None,
            expires_at,
        }
    }

    fn fresh() -> YoutubeCredentials {
        stub_creds(OffsetDateTime::now_utc() + Duration::hours(1))
    }

    fn near_expiry() -> YoutubeCredentials {
        stub_creds(OffsetDateTime::now_utc() + Duration::seconds(60))
    }

    fn refreshed_grant(access: &str) -> StubReply {
        StubReply::ok(&json!({
            "access_token": access,
            "refresh_token": ROTATED_REFRESH,
            "expires_in": 3599,
            "token_type": "Bearer"
        }))
    }

    fn channel_with_handle(handle: &str) -> StubReply {
        StubReply::ok(&json!({
            "items": [{ "id": "UCtest123", "snippet": { "title": "Test Channel", "customUrl": handle } }]
        }))
    }

    struct Rig {
        server: StubServer,
        repo: Arc<InMemRepo>,
        quota: SharedQuota,
        bus: Arc<RecordingPublisher>,
        manager: Arc<YoutubeCredentialsManager>,
    }

    async fn rig(creds: &YoutubeCredentials) -> Rig {
        let server = StubServer::start().await;
        let repo = InMemRepo::with_creds(creds);
        let quota = SharedQuota::default();
        let bus = Arc::new(RecordingPublisher::default());
        let manager = Arc::new(YoutubeCredentialsManager::new(
            repo.clone(),
            oauth_client_at(&server.uri()),
            data_api_with(&server.uri(), quota.clone(), bus.clone()),
        ));
        Rig {
            server,
            repo,
            quota,
            bus,
            manager,
        }
    }

    #[tokio::test]
    async fn load_returns_none_when_no_row() {
        let manager = YoutubeCredentialsManager::new(
            Arc::new(InMemRepo::default()),
            unreachable_oauth_client(),
            unreachable_data_api(),
        );

        assert!(manager.load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn load_of_an_undecryptable_stored_credential_asks_youtube_for_reauth() {
        let manager = YoutubeCredentialsManager::new(
            Arc::new(LoadFailsRepo(|| StorageError::Decryption)),
            unreachable_oauth_client(),
            unreachable_data_api(),
        );

        let err = manager.load().await.unwrap_err();

        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "youtube"),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn save_from_bundle_persists_credentials() {
        let repo = Arc::new(InMemRepo::default());
        let manager = YoutubeCredentialsManager::new(
            repo.clone(),
            unreachable_oauth_client(),
            unreachable_data_api(),
        );
        let bundle = YoutubeAuthBundle {
            access_token: OAuthToken::new("ya29.bundle"),
            refresh_token: OAuthToken::new("1//bundle_refresh"),
            channel_id: "UCbundle".to_owned(),
            channel_title: "Bundle Channel".to_owned(),
            channel_handle: Some("@bundle_channel".to_owned()),
            client_id: "bundle_cid".to_owned(),
            expires_at: SystemTime::now() + StdDuration::from_secs(3600),
        };

        manager.save_from_bundle(bundle).await.unwrap();

        let stored = repo.stored().unwrap();
        assert_eq!(
            (
                stored.access_token.expose(),
                stored.refresh_token.expose(),
                stored.channel_id.as_str(),
                stored.client_id.as_str()
            ),
            ("ya29.bundle", "1//bundle_refresh", "UCbundle", "bundle_cid")
        );
    }

    #[tokio::test]
    async fn the_expiry_watch_follows_a_persist_and_a_clear() {
        let repo = Arc::new(InMemRepo::default());
        let manager = YoutubeCredentialsManager::new(
            repo.clone(),
            unreachable_oauth_client(),
            unreachable_data_api(),
        );
        let expiry = manager.subscribe_expiry();
        let initial = *expiry.borrow();

        manager
            .save_from_bundle(YoutubeAuthBundle {
                access_token: OAuthToken::new("ya29.bundle"),
                refresh_token: OAuthToken::new("1//bundle_refresh"),
                channel_id: "UCbundle".to_owned(),
                channel_title: "Bundle Channel".to_owned(),
                channel_handle: None,
                client_id: "bundle_cid".to_owned(),
                expires_at: SystemTime::now() + StdDuration::from_secs(3600),
            })
            .await
            .unwrap();
        let persisted = repo.stored().unwrap().expires_at;
        let after_persist = *expiry.borrow();
        manager.clear().await.unwrap();
        let after_clear = *expiry.borrow();

        assert_eq!(
            (initial, after_persist, after_clear),
            (None, Some(SystemTime::from(persisted)), None)
        );
    }

    #[tokio::test]
    async fn a_refresh_publishes_the_new_expiry_on_the_watch() {
        let rig = rig(&near_expiry()).await;
        rig.server
            .reply_always(TOKEN_PATH, refreshed_grant(REFRESHED_ACCESS));
        let expiry = rig.manager.subscribe_expiry();

        rig.manager.get_valid_access_token().await.unwrap();

        let persisted = rig.repo.stored().unwrap().expires_at;
        assert_eq!(*expiry.borrow(), Some(SystemTime::from(persisted)));
    }

    #[tokio::test]
    async fn a_refresh_response_without_expires_in_assumes_the_default_lifetime() {
        let rig = rig(&fresh()).await;
        rig.server.reply_always(
            TOKEN_PATH,
            StubReply::ok(&json!({ "access_token": "ya29.new_access", "token_type": "Bearer" })),
        );
        let lifetime = Duration::seconds(DEFAULT_ACCESS_TOKEN_LIFETIME_SECS as i64);

        let before = OffsetDateTime::now_utc();
        rig.manager.refresh(EXISTING_ACCESS).await.unwrap();
        let after = OffsetDateTime::now_utc();

        let expires_at = rig.repo.stored().unwrap().expires_at;
        assert!(
            expires_at >= before + lifetime && expires_at <= after + lifetime,
            "expiry must be refresh time plus the default lifetime"
        );
    }

    #[tokio::test]
    async fn get_valid_access_token_returns_existing_when_fresh() {
        let rig = rig(&fresh()).await;

        let token = rig.manager.get_valid_access_token().await.unwrap();

        assert_eq!(
            (token.as_str(), rig.server.request_count(TOKEN_PATH)),
            (EXISTING_ACCESS, 0)
        );
    }

    #[tokio::test]
    async fn get_valid_access_token_refreshes_and_persists_a_token_near_expiry() {
        let rig = rig(&near_expiry()).await;
        rig.server
            .reply_always(TOKEN_PATH, refreshed_grant(REFRESHED_ACCESS));

        let token = rig.manager.get_valid_access_token().await.unwrap();

        let stored = rig.repo.stored().unwrap();
        assert_eq!(
            (
                token.as_str(),
                stored.access_token.expose(),
                stored.refresh_token.expose()
            ),
            (REFRESHED_ACCESS, REFRESHED_ACCESS, ROTATED_REFRESH)
        );
    }

    #[tokio::test]
    async fn get_valid_access_token_returns_reauth_required_when_no_creds() {
        let manager = YoutubeCredentialsManager::new(
            Arc::new(InMemRepo::default()),
            unreachable_oauth_client(),
            unreachable_data_api(),
        );

        let err = manager.get_valid_access_token().await.unwrap_err();

        assert!(
            matches!(err, PlatformError::ReauthRequired { .. }),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn refresh_keeps_the_stored_refresh_token_when_google_omits_it() {
        let rig = rig(&fresh()).await;
        rig.server.reply_always(
            TOKEN_PATH,
            StubReply::ok(&json!({ "access_token": "ya29.new_access", "expires_in": 3599 })),
        );

        rig.manager.refresh(EXISTING_ACCESS).await.unwrap();

        let stored = rig.repo.stored().unwrap();
        assert_eq!(
            (stored.access_token.expose(), stored.refresh_token.expose()),
            ("ya29.new_access", EXISTING_REFRESH)
        );
    }

    #[tokio::test]
    async fn refresh_of_an_already_replaced_token_returns_the_stored_one_without_a_request() {
        let rig = rig(&fresh()).await;

        let creds = rig.manager.refresh("ya29.long_gone").await.unwrap();

        assert_eq!(
            (
                creds.access_token.expose(),
                rig.server.request_count(TOKEN_PATH)
            ),
            (EXISTING_ACCESS, 0)
        );
    }

    #[tokio::test]
    async fn a_failed_refresh_leaves_the_stored_credentials_untouched() {
        let rig = rig(&near_expiry()).await;
        rig.server.reply_always(
            TOKEN_PATH,
            StubReply::json(400, &json!({ "error": "invalid_grant" })),
        );

        let outcome = rig.manager.get_valid_access_token().await;

        let stored = rig.repo.stored().unwrap();
        assert_eq!(
            (
                matches!(outcome, Err(PlatformError::ReauthRequired { .. })),
                stored.access_token.expose(),
                stored.refresh_token.expose()
            ),
            (true, EXISTING_ACCESS, EXISTING_REFRESH)
        );
    }

    #[tokio::test]
    async fn concurrent_token_requests_and_refreshes_share_one_token_post() {
        let rig = rig(&near_expiry()).await;
        let gate = Arc::new(ReplyGate::default());
        rig.server.reply_times(
            TOKEN_PATH,
            refreshed_grant(REFRESHED_ACCESS).gated_by(&gate),
            1,
        );
        rig.server
            .reply_always(TOKEN_PATH, refreshed_grant(SECOND_ACCESS));
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
            (tokens, rig.server.request_count(TOKEN_PATH)),
            (vec![REFRESHED_ACCESS.to_owned(); CONCURRENT_CALLERS], 1)
        );
    }

    #[tokio::test]
    async fn a_cancelled_waiter_leaves_the_refresh_running_for_the_other_waiters() {
        let rig = rig(&near_expiry()).await;
        let gate = Arc::new(ReplyGate::default());
        rig.server.reply_times(
            TOKEN_PATH,
            refreshed_grant(REFRESHED_ACCESS).gated_by(&gate),
            1,
        );
        rig.server
            .reply_always(TOKEN_PATH, refreshed_grant(SECOND_ACCESS));
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
                rig.server.request_count(TOKEN_PATH)
            ),
            (true, REFRESHED_ACCESS, REFRESHED_ACCESS, ROTATED_REFRESH, 1)
        );
    }

    #[tokio::test]
    async fn clear_during_a_refresh_waits_so_logout_is_never_undone() {
        let rig = rig(&near_expiry()).await;
        let gate = Arc::new(ReplyGate::default());
        rig.server.reply_always(
            TOKEN_PATH,
            refreshed_grant(REFRESHED_ACCESS).gated_by(&gate),
        );
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
    async fn a_refresh_task_that_dies_reports_an_auth_error() {
        let manager = YoutubeCredentialsManager::new(
            Arc::new(PanickingRepo),
            unreachable_oauth_client(),
            unreachable_data_api(),
        );

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

    #[tokio::test]
    async fn the_handle_backfill_persists_the_handle_and_charges_one_unit() {
        let rig = rig(&fresh()).await;
        rig.server
            .reply_always(CHANNELS_PATH, channel_with_handle("@backfilled"));

        let creds = rig.manager.ensure_channel_handle().await.unwrap();

        assert_eq!(
            (
                creds.channel_handle.as_deref(),
                rig.repo.stored().unwrap().channel_handle.as_deref(),
                used_today(&rig.quota).await
            ),
            (Some("@backfilled"), Some("@backfilled"), 1)
        );
    }

    #[tokio::test]
    async fn the_handle_backfill_keeps_the_token_it_just_refreshed() {
        let rig = rig(&near_expiry()).await;
        rig.server
            .reply_always(TOKEN_PATH, refreshed_grant(REFRESHED_ACCESS));
        rig.server
            .reply_always(CHANNELS_PATH, channel_with_handle("@backfilled"));

        rig.manager.ensure_channel_handle().await.unwrap();

        let stored = rig.repo.stored().unwrap();
        assert_eq!(
            (
                stored.access_token.expose(),
                stored.refresh_token.expose(),
                stored.channel_handle.as_deref()
            ),
            (REFRESHED_ACCESS, ROTATED_REFRESH, Some("@backfilled"))
        );
    }

    #[tokio::test]
    async fn a_refused_handle_backfill_keeps_the_credentials_and_publishes_request_fail() {
        for status in [403, 500] {
            let rig = rig(&fresh()).await;
            rig.server.reply_always(
                CHANNELS_PATH,
                StubReply::json(
                    status,
                    &json!({ "error": { "code": status, "errors": [{ "reason": "quotaExceeded" }] } }),
                ),
            );

            let creds = rig.manager.ensure_channel_handle().await.unwrap();

            assert_eq!(
                (
                    creds.channel_handle,
                    rig.repo.stored().unwrap().channel_handle,
                    rig.bus.events_of_kind(REQUEST_FAIL_KIND).len(),
                    used_today(&rig.quota).await
                ),
                (None, None, 1, 1),
                "{status}"
            );
        }
    }

    #[tokio::test]
    async fn a_handle_backfill_for_an_account_without_a_channel_changes_nothing() {
        for body in [json!({}), json!({ "items": [] })] {
            let rig = rig(&fresh()).await;
            rig.server.reply_always(CHANNELS_PATH, StubReply::ok(&body));

            let creds = rig.manager.ensure_channel_handle().await.unwrap();

            assert_eq!(
                (
                    creds.channel_handle,
                    rig.repo.stored().unwrap().channel_handle,
                    rig.server.request_count(CHANNELS_PATH)
                ),
                (None, None, 1),
                "{body}"
            );
        }
    }

    #[tokio::test]
    async fn the_handle_backfill_skips_the_lookup_when_the_handle_is_known() {
        let mut creds = fresh();
        creds.channel_handle = Some("@known".to_owned());
        let rig = rig(&creds).await;

        rig.manager.ensure_channel_handle().await.unwrap();

        assert_eq!(
            (
                rig.server.request_count(CHANNELS_PATH),
                used_today(&rig.quota).await
            ),
            (0, 0)
        );
    }
}
