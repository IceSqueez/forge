use std::sync::Arc;
use std::time::SystemTime;

use async_trait::async_trait;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::OAuthToken;

use crate::auth::TwitchAuthBundle;
use crate::helix::{HelixError, HelixTokenSource};

pub const TWITCH_CREDENTIAL_ID: &str = "twitch:broadcaster";

#[derive(Clone)]
pub struct StoredCredential {
    pub access_token: OAuthToken,
    pub user_id: String,
    pub login: String,
    pub expires_at: Option<SystemTime>,
}

impl std::fmt::Debug for StoredCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredCredential")
            .field("access_token", &self.access_token)
            .field("user_id", &self.user_id)
            .field("login", &self.login)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

pub async fn store(
    creds: &dyn CredentialsRepo,
    auth: &TwitchAuthBundle,
) -> Result<(), StorageError> {
    let expires_at_unix: Option<i64> = auth.expires_at.and_then(|t| {
        t.duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    });
    let bundle = serde_json::json!({
        "access_token": auth.access_token.expose(),
        "user_id": auth.user_info.id,
        "login": auth.user_info.login,
        "expires_at_unix": expires_at_unix,
    });
    creds
        .store(
            &CredentialId::new(TWITCH_CREDENTIAL_ID),
            &bundle.to_string(),
        )
        .await
}

pub async fn load(creds: &dyn CredentialsRepo) -> Result<Option<StoredCredential>, StorageError> {
    let Some(json) = creds.load(&CredentialId::new(TWITCH_CREDENTIAL_ID)).await? else {
        return Ok(None);
    };
    let bundle: serde_json::Value = serde_json::from_str(&json)?;
    let access_token = OAuthToken::new(
        bundle["access_token"]
            .as_str()
            .ok_or_else(|| StorageError::Parse("missing access_token in twitch credential".into()))?
            .to_owned(),
    );
    let user_id = bundle["user_id"]
        .as_str()
        .ok_or_else(|| StorageError::Parse("missing user_id in twitch credential".into()))?
        .to_owned();
    let login = bundle["login"].as_str().unwrap_or_default().to_owned();
    let expires_at = bundle["expires_at_unix"].as_i64().and_then(|secs| {
        if secs <= 0 {
            None
        } else {
            Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64))
        }
    });
    Ok(Some(StoredCredential {
        access_token,
        user_id,
        login,
        expires_at,
    }))
}

pub struct CredentialsTokenSource {
    creds: Arc<dyn CredentialsRepo>,
}

impl CredentialsTokenSource {
    pub fn new(creds: Arc<dyn CredentialsRepo>) -> Self {
        Self { creds }
    }
}

#[async_trait]
impl HelixTokenSource for CredentialsTokenSource {
    async fn access_token(&self) -> Result<OAuthToken, HelixError> {
        let cred = load(self.creds.as_ref())
            .await
            .map_err(|e| HelixError::Credentials(e.to_string()))?
            .ok_or_else(|| HelixError::Credentials("no twitch credentials stored".to_owned()))?;
        Ok(cred.access_token)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn stored_credential_debug_does_not_expose_bearer_token() {
        let cred = StoredCredential {
            access_token: OAuthToken::new("DEADBEEF_BEARER"),
            user_id: "123".to_owned(),
            login: "user".to_owned(),
            expires_at: None,
        };
        assert!(!format!("{cred:?}").contains("DEADBEEF_BEARER"));
    }
}
