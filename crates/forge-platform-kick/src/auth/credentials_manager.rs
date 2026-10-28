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
            return Ok(creds.access_token);
        }
        self.detached_refresh(RefreshTrigger::NearExpiry)
            .await
            .map(|creds| creds.access_token)
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
        RefreshTrigger::Rejected(failed) => existing.access_token != *failed,
    };
    if already_rotated {
        return Ok(existing);
    }

    let grant = oauth.refresh(&existing.refresh_token).await?;
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
