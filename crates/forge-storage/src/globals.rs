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
#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait GlobalsRepo: Send + Sync {
    async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError>;
    async fn set(&self, name: &str, value: Variant, persisted: bool) -> Result<(), StorageError>;
    async fn delete(&self, name: &str) -> Result<bool, StorageError>;

    /// Does not count as a read; lets in-place mutators preserve a global's persistence
    /// without demoting it to session on every edit.
    async fn persisted(&self, name: &str) -> Result<Option<bool>, StorageError> {
        Ok(self
            .list()
            .await?
            .into_iter()
            .find(|e| e.name == name)
            .map(|e| e.persisted))
    }

    /// Use for metadata-only edits instead of `set()`, which always bumps `writes`.
    /// Default impl re-`set()`s the current value and so still bumps `writes` - a real
    /// backend should override for true metadata-only semantics.
    async fn set_persisted(&self, name: &str, persisted: bool) -> Result<bool, StorageError> {
        let Some(entry) = self.list().await?.into_iter().find(|e| e.name == name) else {
            return Ok(false);
        };
        self.set(name, entry.value, persisted).await?;
        Ok(true)
    }

    /// Rejects with [`StorageError::NameCollision`] if `new_name` is already taken, and
    /// [`StorageError::NotFound`] if `old_name` is absent. Default impl is a non-atomic
    /// list/set/delete composition that resets `reads`/`writes`/`created_at`; a real
    /// backend should override with a transactional `UPDATE` of the name column.
    async fn rename(&self, old_name: &str, new_name: &str) -> Result<(), StorageError> {
        if old_name == new_name {
            return Ok(());
        }

        let entries = self.list().await?;
        if entries.iter().any(|e| e.name == new_name) {
            return Err(StorageError::NameCollision {
                name: new_name.to_string(),
            });
        }
        let Some(entry) = entries.into_iter().find(|e| e.name == old_name) else {
            return Err(StorageError::NotFound {
                key: old_name.to_string(),
            });
        };

        self.delete(old_name).await?;
        self.set(new_name, entry.value, entry.persisted).await
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError>;

    /// Soft delete (not [`Self::delete`]): hides `name` from `get`/`list`/`persisted`/
    /// `incr` until [`Self::restore`]; telemetry survives untouched. Default impl has
    /// no generic archived-state representation and returns [`StorageError::NotReady`].
    async fn archive(&self, _name: &str) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Reverses [`Self::archive`]; see its default-impl caveat.
    async fn restore(&self, _name: &str) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Mirror of `list`, which excludes archived entries. Default impl reports none.
    async fn list_archived(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(Vec::new())
    }

    /// Sums ALL globals (persisted and session-scoped alike), not a disk-durable-only figure.
    async fn storage_bytes(&self) -> Result<u64, StorageError>;

    /// Session-only globals never advance this; it tracks `persisted = true` writes only.
    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError>;

    /// Errors with [`StorageError::NotFound`] if absent, or [`StorageError::TypeMismatch`]
    /// if the stored type is not `Int`/`Float`.
    async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError>;

    /// Does not increment `reads` counters; this is an inspection operation, not a
    /// runtime get.
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
