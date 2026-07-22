use std::collections::HashMap;

use async_trait::async_trait;
use forge_types::{ActionId, ExecutionContext};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionStats {
    pub last_ran_at: OffsetDateTime,
    pub runs_24h: u32,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait HistoryRepo: Send + Sync {
    async fn save(&self, ctx: &ExecutionContext) -> Result<(), StorageError>;
    async fn recent_for_action(
        &self,
        action_id: ActionId,
        limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError>;
    /// Only includes actions with at least one history entry.
    async fn stats_summary(
        &self,
        since: OffsetDateTime,
    ) -> Result<HashMap<ActionId, ActionStats>, StorageError>;
    /// Returns rows removed.
    async fn prune_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn HistoryRepo) {}
}
