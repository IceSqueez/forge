use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_storage::{
    BanLedgerEntry, BanLedgerKey, BanLedgerRepo, StorageError, ViewerPlatform, merge_ban_entry,
};
use time::OffsetDateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LedgerOp {
    Upsert,
    Remove,
    Get,
    List,
}

#[derive(Default)]
pub(crate) struct MemoryBanLedger {
    rows: Mutex<HashMap<BanLedgerKey, BanLedgerEntry>>,
    failing: Option<LedgerOp>,
}

impl MemoryBanLedger {
    pub(crate) fn shared() -> Arc<dyn BanLedgerRepo> {
        Arc::new(Self::default())
    }

    pub(crate) fn failing_on(op: LedgerOp) -> Self {
        Self {
            failing: Some(op),
            ..Self::default()
        }
    }

    pub(crate) fn seed(&self, entry: BanLedgerEntry) {
        self.rows().insert(entry.key.clone(), entry);
    }

    pub(crate) fn stored(&self, key: &BanLedgerKey) -> Option<BanLedgerEntry> {
        self.rows().get(key).cloned()
    }

    pub(crate) fn row_count(&self) -> usize {
        self.rows().len()
    }

    fn rows(&self) -> std::sync::MutexGuard<'_, HashMap<BanLedgerKey, BanLedgerEntry>> {
        self.rows
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn check(&self, op: LedgerOp) -> Result<(), StorageError> {
        if self.failing == Some(op) {
            return Err(StorageError::Connection {
                reason: "ban ledger offline".to_owned(),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl BanLedgerRepo for MemoryBanLedger {
    async fn upsert(
        &self,
        entry: &BanLedgerEntry,
        now: OffsetDateTime,
    ) -> Result<BanLedgerEntry, StorageError> {
        self.check(LedgerOp::Upsert)?;
        let mut rows = self.rows();
        let merged = merge_ban_entry(rows.get(&entry.key).cloned(), entry.clone(), now);
        rows.insert(entry.key.clone(), merged.clone());
        Ok(merged)
    }

    async fn remove(&self, key: &BanLedgerKey) -> Result<bool, StorageError> {
        self.check(LedgerOp::Remove)?;
        Ok(self.rows().remove(key).is_some())
    }

    async fn get(
        &self,
        key: &BanLedgerKey,
        now: OffsetDateTime,
    ) -> Result<Option<BanLedgerEntry>, StorageError> {
        self.check(LedgerOp::Get)?;
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
        self.check(LedgerOp::List)?;
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
