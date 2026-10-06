use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_storage::{
    BanLedgerEntry, BanLedgerKey, BanLedgerRepo, StorageError, ViewerPlatform, merge_ban_entry,
};
use time::OffsetDateTime;

#[derive(Default)]
pub(crate) struct MemoryBanLedger {
    rows: Mutex<HashMap<BanLedgerKey, BanLedgerEntry>>,
}

impl MemoryBanLedger {
    pub(crate) fn shared() -> Arc<dyn BanLedgerRepo> {
        Arc::new(Self::default())
    }

    fn rows(&self) -> std::sync::MutexGuard<'_, HashMap<BanLedgerKey, BanLedgerEntry>> {
        self.rows
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[async_trait]
impl BanLedgerRepo for MemoryBanLedger {
    async fn upsert(
        &self,
        entry: &BanLedgerEntry,
        now: OffsetDateTime,
    ) -> Result<BanLedgerEntry, StorageError> {
        let mut rows = self.rows();
        let merged = merge_ban_entry(rows.get(&entry.key).cloned(), entry.clone(), now);
        rows.insert(entry.key.clone(), merged.clone());
        Ok(merged)
    }

    async fn remove(&self, key: &BanLedgerKey) -> Result<bool, StorageError> {
        Ok(self.rows().remove(key).is_some())
    }

    async fn get(
        &self,
        key: &BanLedgerKey,
        now: OffsetDateTime,
    ) -> Result<Option<BanLedgerEntry>, StorageError> {
        Ok(self
            .rows()
            .get(key)
            .filter(|entry| entry.is_active_at(now))
            .cloned())
    }

    async fn list_active(
        &self,
        platform: ViewerPlatform,
        channel_id: &str,
        now: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<BanLedgerEntry>, StorageError> {
        Ok(self
            .rows()
            .values()
            .filter(|entry| {
                entry.key.platform == platform
                    && entry.key.channel_id == channel_id
                    && entry.is_active_at(now)
            })
            .take(limit)
            .cloned()
            .collect())
    }
}
