use async_trait::async_trait;
use forge_types::Variant;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;
use crate::transit::GlobalTransit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEntry {
    pub name: String,
    pub value: Variant,
    pub persisted: bool,
    pub reads: u64,
    pub writes: u64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait GlobalsRepo: Send + Sync {
    async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError>;
    async fn set(&self, name: &str, value: Variant, persisted: bool) -> Result<(), StorageError>;
    async fn delete(&self, name: &str) -> Result<bool, StorageError>;

    async fn peek(&self, name: &str) -> Result<Option<Variant>, StorageError>;

    async fn persisted(&self, name: &str) -> Result<Option<bool>, StorageError>;

    async fn set_persisted(&self, name: &str, persisted: bool) -> Result<bool, StorageError>;

    async fn rename(&self, old_name: &str, new_name: &str) -> Result<(), StorageError>;

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError>;

    async fn count(&self) -> Result<usize, StorageError>;

    async fn archive(&self, name: &str) -> Result<bool, StorageError>;

    async fn restore(&self, name: &str) -> Result<bool, StorageError>;

    async fn list_archived(&self) -> Result<Vec<GlobalEntry>, StorageError>;

    async fn storage_bytes(&self) -> Result<u64, StorageError>;

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError>;

    async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError>;

    async fn array_append(
        &self,
        name: &str,
        item: Variant,
        max_len: Option<usize>,
    ) -> Result<usize, StorageError>;

    async fn array_remove(
        &self,
        name: &str,
        item: &Variant,
        remove_all: bool,
    ) -> Result<usize, StorageError>;

    async fn toggle(&self, name: &str) -> Result<bool, StorageError>;

    async fn export_all(&self) -> Result<Vec<GlobalTransit>, StorageError> {
        let mut entries = self.list().await?;
        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(entries
            .into_iter()
            .map(|e| GlobalTransit {
                name: e.name,
                value: e.value,
                persisted: e.persisted,
                last_modified: e.last_modified,
                reads: e.reads,
                writes: e.writes,
            })
            .collect())
    }
}

pub fn append_bounded(items: &mut Vec<Variant>, item: Variant, max_len: Option<usize>) {
    items.push(item);
    if let Some(max_len) = max_len
        && items.len() > max_len
    {
        let excess = items.len() - max_len;
        items.drain(..excess);
    }
}

pub fn remove_matching(items: &mut Vec<Variant>, target: &Variant, remove_all: bool) {
    if remove_all {
        items.retain(|item| item != target);
    } else if let Some(pos) = items.iter().position(|item| item == target) {
        items.remove(pos);
    }
}

pub fn type_mismatch(name: &str, found: &Variant) -> StorageError {
    StorageError::TypeMismatch {
        name: name.to_string(),
        actual: found.type_tag().to_string(),
    }
}
