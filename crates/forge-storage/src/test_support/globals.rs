#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use forge_types::{RecoverMutex, Variant, VariantKind};
use time::OffsetDateTime;

use crate::{GlobalEntry, GlobalsRepo, StorageError};

#[derive(Default)]
pub struct MemoryGlobals {
    rows: Mutex<BTreeMap<String, (Variant, bool)>>,
    unreadable: Option<String>,
}

impl MemoryGlobals {
    pub fn with<'a>(entries: impl IntoIterator<Item = (&'a str, Variant)>) -> Self {
        Self::seeded(
            entries
                .into_iter()
                .map(|(name, value)| (name, value, false)),
        )
    }

    pub fn seeded<'a>(entries: impl IntoIterator<Item = (&'a str, Variant, bool)>) -> Self {
        let rows = entries
            .into_iter()
            .map(|(name, value, persisted)| (name.to_owned(), (value, persisted)))
            .collect();
        Self {
            rows: Mutex::new(rows),
            unreadable: None,
        }
    }

    #[must_use]
    pub fn failing_reads_of(mut self, name: &str) -> Self {
        self.unreadable = Some(name.to_owned());
        self
    }

    pub fn snapshot(&self, name: &str) -> Option<Variant> {
        self.rows().get(name).map(|(value, _)| value.clone())
    }

    pub fn persisted_flag(&self, name: &str) -> Option<bool> {
        self.rows().get(name).map(|(_, persisted)| *persisted)
    }

    pub fn array(&self, name: &str) -> Vec<Variant> {
        match self.snapshot(name) {
            Some(Variant::Array(items)) => items,
            other => panic!("expected an array at {name}, got {other:?}"),
        }
    }

    fn rows(&self) -> MutexGuard<'_, BTreeMap<String, (Variant, bool)>> {
        self.rows.lock_recover()
    }
}

#[async_trait]
impl GlobalsRepo for MemoryGlobals {
    async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
        if self.unreadable.as_deref() == Some(name) {
            return Err(StorageError::Decryption);
        }
        Ok(self.snapshot(name))
    }

    async fn set(&self, name: &str, value: Variant, persisted: bool) -> Result<(), StorageError> {
        self.rows().insert(name.to_owned(), (value, persisted));
        Ok(())
    }

    async fn delete(&self, name: &str) -> Result<bool, StorageError> {
        Ok(self.rows().remove(name).is_some())
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(self
            .rows()
            .iter()
            .map(|(name, (value, persisted))| GlobalEntry {
                name: name.clone(),
                value: value.clone(),
                persisted: *persisted,
                reads: 0,
                writes: 0,
                created_at: OffsetDateTime::UNIX_EPOCH,
                last_modified: OffsetDateTime::UNIX_EPOCH,
            })
            .collect())
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError> {
        let mut rows = self.rows();
        let Some((current, persisted)) = rows.get(name).cloned() else {
            return Err(StorageError::NotFound {
                key: name.to_owned(),
            });
        };
        let next = match current {
            Variant::Int(value) => Variant::Int(value + amount),
            Variant::Float(value) => {
                Variant::float(value + amount as f64).expect("a finite test global")
            }
            other => {
                return Err(StorageError::TypeMismatch {
                    name: name.to_owned(),
                    actual: VariantKind::from_variant(&other).label().to_owned(),
                });
            }
        };
        rows.insert(name.to_owned(), (next.clone(), persisted));
        Ok(next)
    }
}
