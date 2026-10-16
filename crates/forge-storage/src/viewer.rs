use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use forge_types::PlatformId;

use crate::StorageError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Viewer {
    pub viewer_id: String,
    pub platform: PlatformId,
    pub username: String,
    pub first_seen_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
    pub custom_greeting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerMessage {
    pub platform: PlatformId,
    pub viewer_id: String,
    pub username: String,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait ViewerRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<Viewer>, StorageError>;
    async fn count(&self) -> Result<u64, StorageError>;
    async fn get(
        &self,
        platform: PlatformId,
        viewer_id: &str,
    ) -> Result<Option<Viewer>, StorageError>;
    async fn record_message(
        &self,
        platform: PlatformId,
        viewer_id: &str,
        username: &str,
    ) -> Result<(), StorageError>;
    async fn record_messages(&self, messages: &[ViewerMessage]) -> Result<(), StorageError> {
        for message in messages {
            self.record_message(message.platform, &message.viewer_id, &message.username)
                .await?;
        }
        Ok(())
    }
    async fn set_custom_greeting(
        &self,
        platform: PlatformId,
        viewer_id: &str,
        enabled: bool,
    ) -> Result<bool, StorageError>;
}
