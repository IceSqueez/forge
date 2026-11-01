#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use forge_types::{RecoverMutex, Variant, VariantKind};
use time::OffsetDateTime;

use crate::globals::{append_bounded, remove_matching, type_mismatch};
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

    async fn peek(&self, name: &str) -> Result<Option<Variant>, StorageError> {
        self.get(name).await
    }

    async fn persisted(&self, name: &str) -> Result<Option<bool>, StorageError> {
        Ok(self.rows().get(name).map(|(_, persisted)| *persisted))
    }

    async fn set_persisted(&self, name: &str, persisted: bool) -> Result<bool, StorageError> {
        Ok(self
            .rows()
            .get_mut(name)
            .map(|row| row.1 = persisted)
            .is_some())
    }

    async fn rename(&self, old_name: &str, new_name: &str) -> Result<(), StorageError> {
        if old_name == new_name {
            return Ok(());
        }
        let mut rows = self.rows();
        if rows.contains_key(new_name) {
            return Err(StorageError::NameCollision {
                name: new_name.to_owned(),
            });
        }
        let Some(row) = rows.remove(old_name) else {
            return Err(StorageError::NotFound {
                key: old_name.to_owned(),
            });
        };
        rows.insert(new_name.to_owned(), row);
        Ok(())
    }

    async fn rename_and_set(
        &self,
        old_name: &str,
        new_name: &str,
        value: Variant,
        persisted: bool,
    ) -> Result<(), StorageError> {
        let mut rows = self.rows();
        if old_name != new_name && rows.contains_key(new_name) {
            return Err(StorageError::NameCollision {
                name: new_name.to_owned(),
            });
        }
        if rows.remove(old_name).is_none() {
            return Err(StorageError::NotFound {
                key: old_name.to_owned(),
            });
        }
        rows.insert(new_name.to_owned(), (value, persisted));
        Ok(())
    }

    async fn count(&self) -> Result<usize, StorageError> {
        Ok(self.rows().len())
    }

    async fn archive(&self, _name: &str) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn restore(&self, _name: &str) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn list_archived(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(Vec::new())
    }

    async fn array_append(
        &self,
        name: &str,
        item: Variant,
        max_len: Option<usize>,
    ) -> Result<usize, StorageError> {
        let mut items = match self.get(name).await? {
            None => Vec::new(),
            Some(Variant::Array(items)) => items,
            Some(other) => return Err(type_mismatch(name, &other)),
        };
        append_bounded(&mut items, item, max_len);
        let len = items.len();
        let persisted = self.persisted(name).await?.unwrap_or(false);
        self.set(name, Variant::Array(items), persisted).await?;
        Ok(len)
    }

    async fn array_remove(
        &self,
        name: &str,
        item: &Variant,
        remove_all: bool,
    ) -> Result<usize, StorageError> {
        let mut items = match self.get(name).await? {
            None => {
                return Err(StorageError::NotFound {
                    key: name.to_owned(),
                });
            }
            Some(Variant::Array(items)) => items,
            Some(other) => return Err(type_mismatch(name, &other)),
        };
        remove_matching(&mut items, item, remove_all);
        let len = items.len();
        let persisted = self.persisted(name).await?.unwrap_or(false);
        self.set(name, Variant::Array(items), persisted).await?;
        Ok(len)
    }

    async fn toggle(&self, name: &str) -> Result<bool, StorageError> {
        let flipped = match self.get(name).await? {
            None => {
                return Err(StorageError::NotFound {
                    key: name.to_owned(),
                });
            }
            Some(Variant::Bool(current)) => !current,
            Some(other) => return Err(type_mismatch(name, &other)),
        };
        let persisted = self.persisted(name).await?.unwrap_or(false);
        self.set(name, Variant::Bool(flipped), persisted).await?;
        Ok(flipped)
    }
}
