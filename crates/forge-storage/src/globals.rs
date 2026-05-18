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

/// `reads` and `writes` increment transactionally with every `get`/`set` call.
#[async_trait]
pub trait GlobalsRepo: Send + Sync {
    async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError>;
    async fn set(&self, name: &str, value: Variant, persisted: bool) -> Result<(), StorageError>;
    async fn delete(&self, name: &str) -> Result<bool, StorageError>;
    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError>;
    async fn storage_bytes(&self) -> Result<u64, StorageError>;
    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError>;

    /// Atomically adds `amount` to an `Int` or `Float` global and returns the updated value.
    ///
    /// Errors with [`StorageError::NotFound`] if the key does not exist, or
    /// [`StorageError::TypeMismatch`] if the stored type is not numeric.
    async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError>;

    /// Returns all globals in transit shape for export, sorted by name.
    ///
    /// Does not increment `reads` counters — this is an inspection operation,
    /// not a runtime get. Backends with a more efficient bulk path may override.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn GlobalsRepo) {}

    #[test]
    fn global_entry_serde_roundtrip() {
        let entry = GlobalEntry {
            name: "my_counter".to_owned(),
            value: Variant::Int(42),
            persisted: true,
            reads: 7,
            writes: 3,
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
            last_modified: OffsetDateTime::from_unix_timestamp(1_700_001_000).unwrap(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let decoded: GlobalEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.name, entry.name);
        assert_eq!(decoded.persisted, entry.persisted);
        assert_eq!(decoded.reads, entry.reads);
        assert_eq!(decoded.writes, entry.writes);
        assert_eq!(decoded.created_at, entry.created_at);
        assert_eq!(decoded.last_modified, entry.last_modified);

        match (decoded.value, entry.value) {
            (Variant::Int(a), Variant::Int(b)) => assert_eq!(a, b),
            _ => panic!("variant mismatch after roundtrip"),
        }
    }
}
