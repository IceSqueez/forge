use async_trait::async_trait;
use forge_types::unified_chat::UnifiedChatRow;

use crate::StorageError;

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait ChatHistoryRepo: Send + Sync {
    async fn append(&self, row: &UnifiedChatRow) -> Result<(), StorageError>;

    /// Returns up to `limit` rows ordered newest-first.
    async fn list_recent(&self, limit: usize) -> Result<Vec<UnifiedChatRow>, StorageError>;

    /// Deletes all rows outside the newest `max_rows` (ordered by `received_at`).
    /// Returns the number of rows deleted.
    async fn prune_to_limit(&self, max_rows: usize) -> Result<u64, StorageError>;
}
