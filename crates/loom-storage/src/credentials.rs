use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::StorageError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialId(pub String);

impl CredentialId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CredentialId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[async_trait]
pub trait CredentialsRepo: Send + Sync {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError>;

    /// Returns None if the credential is not present.
    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError>;

    /// Returns true if a credential was actually removed.
    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError>;

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError>;

    async fn last_refresh(&self, id: &CredentialId)
    -> Result<Option<OffsetDateTime>, StorageError>;

    async fn mark_refreshed(&self, id: &CredentialId) -> Result<(), StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn credential_id_serde_roundtrip() {
        let id = CredentialId::new("twitch:broadcaster");
        let json = serde_json::to_string(&id).unwrap();
        let restored: CredentialId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, restored);
    }

    #[test]
    fn credential_id_transparent_serde() {
        let id = CredentialId::new("azure:tts");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, r#""azure:tts""#);
    }

    #[test]
    fn credential_id_display_matches_inner() {
        assert_eq!(format!("{}", CredentialId::new("twitch:bot")), "twitch:bot");
    }

    #[test]
    fn credentials_repo_is_dyn_safe() {
        fn accepts_repo(_: &dyn CredentialsRepo) {}
        let _ = accepts_repo;
    }
}
