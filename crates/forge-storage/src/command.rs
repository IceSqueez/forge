use async_trait::async_trait;
use forge_types::{Command, CommandId};

use crate::StorageError;

#[async_trait]
pub trait CommandRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<Command>, StorageError>;
    async fn get_by_name(&self, name: &str) -> Result<Option<Command>, StorageError>;
    async fn save(&self, command: &Command) -> Result<(), StorageError>;
    /// Returns true if a row was removed.
    async fn delete(&self, id: CommandId) -> Result<bool, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn CommandRepo) {}
}
