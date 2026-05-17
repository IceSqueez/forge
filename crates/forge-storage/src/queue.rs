use async_trait::async_trait;
use forge_types::{Queue, QueueId};

use crate::StorageError;

#[async_trait]
pub trait QueueRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<Queue>, StorageError>;
    async fn get(&self, id: QueueId) -> Result<Option<Queue>, StorageError>;
    async fn get_by_name(&self, name: &str) -> Result<Option<Queue>, StorageError>;
    async fn save(&self, queue: &Queue) -> Result<(), StorageError>;
    /// Returns true if a row was removed.
    async fn delete(&self, id: QueueId) -> Result<bool, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn QueueRepo) {}
}
