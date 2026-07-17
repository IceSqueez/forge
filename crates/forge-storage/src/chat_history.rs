use async_trait::async_trait;
use forge_types::unified_chat::{ChatSource, UnifiedChatRow};

use crate::StorageError;

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait ChatHistoryRepo: Send + Sync {
    async fn append(&self, row: &UnifiedChatRow) -> Result<(), StorageError>;

    /// Returns up to `limit` rows ordered newest-first.
    async fn list_recent(&self, limit: usize) -> Result<Vec<UnifiedChatRow>, StorageError>;

    /// Deletes all rows outside the newest `max_rows` by insertion order.
    /// Returns the number of rows deleted.
    async fn prune_to_limit(&self, max_rows: usize) -> Result<u64, StorageError>;

    /// Marks the row for `platform_msg_id` as deleted. Returns rows affected (0 or 1).
    async fn mark_message_deleted(&self, platform_msg_id: &str) -> Result<u64, StorageError>;

    /// Marks every stored message from `author` on `source` as deleted, and as
    /// timed out when `timeout` else banned. Returns rows affected.
    async fn mark_user_messages_moderated(
        &self,
        source: ChatSource,
        author: &str,
        timeout: bool,
    ) -> Result<u64, StorageError>;

    /// Marks every stored message on `source` as deleted. Returns rows affected.
    async fn clear_platform(&self, source: ChatSource) -> Result<u64, StorageError>;
}
