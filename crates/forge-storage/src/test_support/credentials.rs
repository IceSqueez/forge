use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::{CredentialId, CredentialsRepo, StorageError};

#[derive(Default)]
pub struct MemoryCredentials {
    rows: Mutex<BTreeMap<String, String>>,
}

impl MemoryCredentials {
    pub fn holding(id: &str, plaintext: &str) -> Self {
        let credentials = Self::default();
        credentials.put(id, plaintext);
        credentials
    }

    pub fn put(&self, id: &str, plaintext: &str) {
        self.rows().insert(id.to_owned(), plaintext.to_owned());
    }

    pub fn stored(&self, id: &str) -> Option<String> {
        self.rows().get(id).cloned()
    }

    pub fn ids(&self) -> Vec<String> {
        self.rows().keys().cloned().collect()
    }

    fn rows(&self) -> MutexGuard<'_, BTreeMap<String, String>> {
        self.rows.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl CredentialsRepo for MemoryCredentials {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        self.put(id.as_str(), plaintext_bundle);
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.stored(id.as_str()))
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.rows().remove(id.as_str()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(self.ids().into_iter().map(CredentialId::new).collect())
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
