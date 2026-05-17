use async_trait::async_trait;
use forge_types::{Action, ActionId};

use crate::StorageError;

#[async_trait]
pub trait ActionRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<Action>, StorageError>;
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError>;
    async fn save(&self, action: &Action) -> Result<(), StorageError>;
    /// Returns true if a row was removed.
    async fn delete(&self, id: ActionId) -> Result<bool, StorageError>;
    async fn list_by_group(&self, group: Option<&str>) -> Result<Vec<Action>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn ActionRepo) {}
}
