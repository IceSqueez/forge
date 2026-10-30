use async_trait::async_trait;
use forge_types::{Queue, QueueId};
use serde::{Deserialize, Serialize};

use crate::StorageError;

pub const OVERLAY_ALERT_QUEUE_NAME: &str = "Overlay Alerts";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueRole {
    OverlayAlerts,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait QueueRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<Queue>, StorageError>;
    async fn get(&self, id: QueueId) -> Result<Option<Queue>, StorageError>;
    async fn get_by_name(&self, name: &str) -> Result<Option<Queue>, StorageError>;
    async fn save(&self, queue: &Queue) -> Result<(), StorageError>;
    async fn delete(&self, id: QueueId) -> Result<bool, StorageError>;
    async fn delete_reassigning_actions(&self, id: QueueId) -> Result<Option<u64>, StorageError>;

    async fn overlay_alert_queue(
        &self,
        adoptable_name: &str,
    ) -> Result<Option<Queue>, StorageError> {
        self.get_by_name(adoptable_name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn QueueRepo) {}
}
