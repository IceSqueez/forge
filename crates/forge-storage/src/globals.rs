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

    /// Stored `persisted` flag for `name`, or `None` when the key is absent.
    ///
    /// Lets in-place mutators (toggle, array append/remove) keep a global's
    /// persistence instead of demoting it to session on every edit. Does not
    /// count as a read. Backends with a direct lookup may override.
    async fn persisted(&self, name: &str) -> Result<Option<bool>, StorageError> {
        Ok(self
            .list()
            .await?
            .into_iter()
            .find(|e| e.name == name)
            .map(|e| e.persisted))
    }

    /// Updates only the `persisted` flag for `name`; does not touch `writes`. Returns
    /// `false` when `name` does not exist.
    ///
    /// Use this for metadata-only edits (e.g. a UI persist toggle) instead of `set()`,
    /// which always bumps `writes` even when the value itself is unchanged. Backends
    /// without a direct column update inherit this default, which re-`set()`s the
    /// current value and so still bumps `writes` — a real backend should override for
    /// true metadata-only semantics.
    async fn set_persisted(&self, name: &str, persisted: bool) -> Result<bool, StorageError> {
        let Some(entry) = self.list().await?.into_iter().find(|e| e.name == name) else {
            return Ok(false);
        };
        self.set(name, entry.value, persisted).await?;
        Ok(true)
    }

    /// Renames `old_name` to `new_name`, preserving the row's value, `persisted`
    /// flag, and `reads`/`writes`/`created_at` telemetry — only the name (the
    /// join key scripts and sub-actions reference) changes.
    ///
    /// Rejects with [`StorageError::NameCollision`] when `new_name` already
    /// names a different global (never silently overwrites it), and with
    /// [`StorageError::NotFound`] when `old_name` does not exist. Renaming a
    /// name to itself is a no-op success.
    ///
    /// The default impl is a best-effort composition of `list`/`set`/`delete`
    /// for backends without a direct column update; it is **not** atomic and
    /// does **not** preserve `reads`/`writes`/`created_at` (the re-inserted row
    /// starts fresh). A real backend should override this with a single
    /// transactional `UPDATE` of the name column to get both properties.
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

    /// Sums the footprint of ALL globals, persisted and session-scoped alike — the
    /// Data screen's total storage-used figure, not a disk-durable-only figure.
    async fn storage_bytes(&self) -> Result<u64, StorageError>;

    /// Timestamp of the most recent write to a `persisted = true` global. Session-only
    /// globals are never durably saved, so they never advance this value.
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
