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
    /// Absent routes the first expiry to re-auth (no refresh possible).
    pub refresh_token: Option<OAuthToken>,
    pub user_id: String,
    pub login: String,
    pub expires_at: Option<SystemTime>,
}

impl std::fmt::Debug for StoredCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredCredential")
            .field("access_token", &self.access_token)
            .field("refresh_token", &self.refresh_token)
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
    let stored = StoredCredential {
        access_token: auth.access_token.clone(),
        refresh_token: auth.refresh_token.clone(),
        user_id: auth.user_info.id.clone(),
        login: auth.user_info.login.clone(),
        expires_at: auth.expires_at,
    };
    store_credential(creds, &stored).await
}

pub async fn store_credential(
    creds: &dyn CredentialsRepo,
    cred: &StoredCredential,
) -> Result<(), StorageError> {
    let expires_at_unix: Option<i64> = cred.expires_at.and_then(|t| {
        t.duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    });
    let bundle = serde_json::json!({
        "access_token": cred.access_token.expose(),
        "refresh_token": cred.refresh_token.as_ref().map(OAuthToken::expose),
        "user_id": cred.user_id,
        "login": cred.login,
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
    let refresh_token = bundle["refresh_token"]
        .as_str()
        .map(|s| OAuthToken::new(s.to_owned()));
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
        refresh_token,
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
    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use time::OffsetDateTime;

    use super::*;

    struct InMemRepo(Mutex<HashMap<String, String>>);

    impl InMemRepo {
        fn empty() -> Self {
            Self(Mutex::new(HashMap::new()))
        }

        fn with_raw(key: &str, value: &str) -> Self {
            let mut map = HashMap::new();
            map.insert(key.to_owned(), value.to_owned());
            Self(Mutex::new(map))
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

    fn cred_with_refresh(refresh: Option<&str>) -> StoredCredential {
        StoredCredential {
            access_token: OAuthToken::new("access_token_value"),
            refresh_token: refresh.map(|s| OAuthToken::new(s.to_owned())),
            user_id: "user_42".to_owned(),
            login: "streamer".to_owned(),
            expires_at: Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(9_999_999_999)),
        }
    }

    #[test]
    fn stored_credential_debug_does_not_expose_access_or_refresh_tokens() {
        let cred = cred_with_refresh(Some("DEADBEEF_REFRESH_SECRET"));
        let debug = format!("{cred:?}");
        assert!(
            !debug.contains("access_token_value"),
            "access_token leaked: {debug}"
        );
        assert!(
            !debug.contains("DEADBEEF_REFRESH_SECRET"),
            "refresh_token leaked: {debug}"
        );
        assert!(
            debug.contains("<redacted>"),
            "expected <redacted> marker: {debug}"
        );
    }

    #[tokio::test]
    async fn store_load_round_trip_preserves_refresh_token() {
        let repo = InMemRepo::empty();
        let original = cred_with_refresh(Some("rt_secret_value"));

        store_credential(&repo, &original).await.unwrap();
        let loaded = load(&repo).await.unwrap().unwrap();

        let rt = loaded.refresh_token.unwrap();
        assert_eq!(rt.expose(), "rt_secret_value");
        assert_eq!(loaded.access_token.expose(), "access_token_value");
        assert_eq!(loaded.user_id, "user_42");
        assert_eq!(loaded.login, "streamer");
    }

    #[tokio::test]
    async fn store_load_round_trip_without_refresh_token_loads_as_none() {
        let repo = InMemRepo::empty();
        let original = cred_with_refresh(None);

        store_credential(&repo, &original).await.unwrap();
        let loaded = load(&repo).await.unwrap().unwrap();

        assert!(
            loaded.refresh_token.is_none(),
            "refresh_token absent on store must load as None"
        );
    }

    #[tokio::test]
    async fn legacy_blob_without_refresh_token_loads_as_none_not_error() {
        let legacy_json = r#"{"access_token":"old_access","user_id":"u1","login":"streamer","expires_at_unix":9999999999}"#;
        let repo = InMemRepo::with_raw(TWITCH_CREDENTIAL_ID, legacy_json);

        let loaded = load(&repo).await.unwrap().unwrap();
        assert!(
            loaded.refresh_token.is_none(),
            "legacy blob without refresh_token must deserialise to None, not error"
        );
        assert_eq!(loaded.access_token.expose(), "old_access");
    }

    #[tokio::test]
    async fn blob_with_null_refresh_token_loads_as_none() {
        let json = r#"{"access_token":"at","refresh_token":null,"user_id":"u1","login":"x","expires_at_unix":9999999999}"#;
        let repo = InMemRepo::with_raw(TWITCH_CREDENTIAL_ID, json);

        let loaded = load(&repo).await.unwrap().unwrap();
        assert!(loaded.refresh_token.is_none());
    }

    #[tokio::test]
    async fn blob_with_zero_expires_at_loads_as_none() {
        let json = r#"{"access_token":"at","user_id":"u1","login":"x","expires_at_unix":0}"#;
        let repo = InMemRepo::with_raw(TWITCH_CREDENTIAL_ID, json);

        let loaded = load(&repo).await.unwrap().unwrap();
        assert!(
            loaded.expires_at.is_none(),
            "expires_at_unix = 0 must decode to None (no expiry)"
        );
    }

    #[tokio::test]
    async fn load_returns_none_when_no_row_exists() {
        let repo = InMemRepo::empty();
        let result = load(&repo).await.unwrap();
        assert!(result.is_none());
    }
}
