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
    async fn save_batch(&self, contexts: &[ExecutionContext]) -> Result<(), StorageError> {
        for ctx in contexts {
            self.save(ctx).await?;
        }
        Ok(())
    }
    async fn recent_for_action(
        &self,
        action_id: ActionId,
        limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError>;
    async fn recent_for_builtin(
        &self,
        builtin_id: &str,
        limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError>;
    async fn recent(&self, limit: u32) -> Result<Vec<ExecutionContext>, StorageError> {
        let _ = limit;
        Ok(Vec::new())
    }
    async fn stats_summary(
        &self,
        since: OffsetDateTime,
    ) -> Result<HashMap<ActionId, ActionStats>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    struct MinimalRepo;

    #[async_trait]
    impl HistoryRepo for MinimalRepo {
        async fn save(&self, _ctx: &ExecutionContext) -> Result<(), StorageError> {
            Ok(())
        }
        async fn recent_for_action(
            &self,
            _action_id: ActionId,
            _limit: u32,
        ) -> Result<Vec<ExecutionContext>, StorageError> {
            Ok(Vec::new())
        }
        async fn recent_for_builtin(
            &self,
            _builtin_id: &str,
            _limit: u32,
        ) -> Result<Vec<ExecutionContext>, StorageError> {
            Ok(Vec::new())
        }
        async fn stats_summary(
            &self,
            _since: OffsetDateTime,
        ) -> Result<HashMap<ActionId, ActionStats>, StorageError> {
            Ok(HashMap::new())
        }
        async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn recent_defaults_to_an_empty_history_for_a_repo_that_does_not_override_it() {
        let repo: &dyn HistoryRepo = &MinimalRepo;

        for limit in [0, 1, u32::MAX] {
            assert_eq!(
                repo.recent(limit).await.unwrap(),
                Vec::new(),
                "limit {limit}",
            );
        }
    }
}
