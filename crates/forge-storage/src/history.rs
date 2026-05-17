use async_trait::async_trait;
use forge_types::{ActionId, ExecutionContext};

use crate::StorageError;

#[async_trait]
pub trait HistoryRepo: Send + Sync {
    async fn save(&self, ctx: &ExecutionContext) -> Result<(), StorageError>;
    async fn recent_for_action(
        &self,
        action_id: ActionId,
        limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn HistoryRepo) {}
}
