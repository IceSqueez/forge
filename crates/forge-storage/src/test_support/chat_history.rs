use std::sync::Arc;

use async_trait::async_trait;
use forge_types::{PlatformId, UnifiedChatRow, UserRemoval};

use crate::{
    ChatAuthorKey, ChatAuthorPage, ChatAuthorTally, ChatHistoryCursor, ChatHistoryRepo,
    ChatModerationTarget, StorageError,
};

#[async_trait]
pub trait ChatHistoryScript: Send + Sync {
    async fn append_rows(&self, rows: &[UnifiedChatRow]) -> Result<(), StorageError>;

    async fn retain(&self) -> Result<u64, StorageError> {
        Ok(0)
    }
}

pub struct ScriptedChatHistoryRepo<S>(pub Arc<S>);

#[async_trait]
impl<S: ChatHistoryScript> ChatHistoryRepo for ScriptedChatHistoryRepo<S> {
    async fn append(&self, row: &UnifiedChatRow) -> Result<(), StorageError> {
        self.0.append_rows(std::slice::from_ref(row)).await
    }

    async fn append_batch(&self, rows: &[UnifiedChatRow]) -> Result<(), StorageError> {
        self.0.append_rows(rows).await
    }

    async fn list_recent(&self, _: usize) -> Result<Vec<UnifiedChatRow>, StorageError> {
        Ok(Vec::new())
    }

    async fn author_page(
        &self,
        _: &ChatAuthorKey,
        _: Option<ChatHistoryCursor>,
        _: usize,
    ) -> Result<ChatAuthorPage, StorageError> {
        Ok(ChatAuthorPage::default())
    }

    async fn author_tallies(
        &self,
        _: &[ChatAuthorKey],
    ) -> Result<Vec<ChatAuthorTally>, StorageError> {
        Ok(Vec::new())
    }

    async fn apply_retention(&self, _: &[ChatAuthorKey], _: usize) -> Result<u64, StorageError> {
        self.0.retain().await
    }

    async fn mark_message_deleted(&self, _: PlatformId, _: &str) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn mark_user_messages_moderated(
        &self,
        _: &ChatModerationTarget,
        _: UserRemoval,
    ) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn clear_platform(&self, _: PlatformId) -> Result<u64, StorageError> {
        Ok(0)
    }
}
