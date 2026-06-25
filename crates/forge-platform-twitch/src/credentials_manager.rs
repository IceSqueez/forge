use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_storage::{CredentialsRepo, StorageError};
use forge_types::OAuthToken;
use serde::Deserialize;

use crate::auth::TWITCH_TOKEN_ENDPOINT;
use crate::credentials::{StoredCredential, load, store_credential};

const PLATFORM: &str = "twitch";
const REFRESH_BUFFER: Duration = Duration::from_secs(5 * 60);

pub struct TwitchCredentialsManager {
    repo: Arc<dyn CredentialsRepo>,
    client: reqwest::Client,
    client_id: String,
    refresh_endpoint: String,
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
        Self {
            repo,
            client: reqwest::Client::new(),
            client_id,
            refresh_endpoint,
        }
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

    /// Public-client `grant_type=refresh_token` POST — no `client_secret`.
    /// Persists the rotated refresh token Twitch returns and invalidates the
    /// old one; the prior token is kept only when the response omits a new one.
    /// A 400/401 means the refresh token itself is rejected → re-auth.
    pub async fn refresh(
        &self,
        refresh_token: &OAuthToken,
    ) -> Result<StoredCredential, PlatformError> {
        let existing = self.load().await?.ok_or_else(reauth_err)?;

        let response = self
            .client
            .post(&self.refresh_endpoint)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.expose()),
                ("client_id", self.client_id.as_str()),
            ])
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            if status == 400 || status == 401 {
                return Err(reauth_err());
            }
            return Err(PlatformError::Http { status, body });
        }

        let body = response.text().await.map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;
        let parsed: RefreshResponse = serde_json::from_str(&body)?;

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

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    expires_in: Option<u64>,
    refresh_token: Option<String>,
}
