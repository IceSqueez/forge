use std::fmt;

use async_trait::async_trait;
use forge_types::redaction::Redacted;
use forge_types::unified_chat::{ChatSource, UnifiedChatRow};

use crate::StorageError;

pub const AUTHORLESS_CHAT_HISTORY_RETAINED: usize = 2000;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ChatAuthorKey {
    pub source: ChatSource,
    pub author_id: String,
}

impl fmt::Debug for ChatAuthorKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatAuthorKey")
            .field("source", &self.source)
            .field("author_id", &Redacted)
            .finish()
    }
}

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

    async fn list_recent_messages_by_author(
        &self,
        author: &ChatAuthorKey,
        limit: usize,
    ) -> Result<Vec<UnifiedChatRow>, StorageError>;

    async fn apply_retention(
        &self,
        authors: &[ChatAuthorKey],
        per_author: usize,
    ) -> Result<u64, StorageError>;

    async fn mark_message_deleted(&self, platform_msg_id: &str) -> Result<u64, StorageError>;

    async fn mark_user_messages_moderated(
        &self,
        source: ChatSource,
        author: &str,
        timeout: bool,
    ) -> Result<u64, StorageError>;

    async fn clear_platform(&self, source: ChatSource) -> Result<u64, StorageError>;
}

#[cfg(test)]
mod tests {
    use forge_types::unified_chat::ChatSource;

    use super::ChatAuthorKey;

    #[test]
    fn chat_author_key_debug_hides_the_author_id_but_keeps_the_source() {
        let key = ChatAuthorKey {
            source: ChatSource::Kick,
            author_id: "SENTINEL_VIEWER_ID".to_owned(),
        };

        let rendered = format!("{key:?}");

        assert!(!rendered.contains("SENTINEL"), "{rendered}");
        assert!(rendered.contains("Kick"), "{rendered}");
    }
}
