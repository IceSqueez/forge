use async_trait::async_trait;
use forge_types::unified_chat::{ChatSource, UnifiedChatRow};

use crate::StorageError;

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait ChatHistoryRepo: Send + Sync {
    async fn append(&self, row: &UnifiedChatRow) -> Result<(), StorageError>;

    async fn append_batch(&self, rows: &[UnifiedChatRow]) -> Result<(), StorageError> {
        for row in rows {
            self.append(row).await?;
        }
        Ok(())
    }

    async fn list_recent(&self, limit: usize) -> Result<Vec<UnifiedChatRow>, StorageError>;

    async fn prune_to_limit(&self, max_rows: usize) -> Result<u64, StorageError>;

    async fn mark_message_deleted(&self, platform_msg_id: &str) -> Result<u64, StorageError>;

    async fn mark_user_messages_moderated(
        &self,
        source: ChatSource,
        author: &str,
        timeout: bool,
    ) -> Result<u64, StorageError>;

    async fn clear_platform(&self, source: ChatSource) -> Result<u64, StorageError>;
}
