use std::collections::HashMap;

use async_trait::async_trait;
use forge_types::{ActionId, ExecutionContext};
use time::OffsetDateTime;

use crate::settings::reserved_keys;
use crate::{MAX_EVENT_LOG_RETENTION_DAYS, SettingsRepo, StorageError, event_log_retention_days};

pub const MIN_ACTION_HISTORY_RETENTION_DAYS: u32 = 7;
pub const MAX_ACTION_HISTORY_RETENTION_DAYS: u32 = MAX_EVENT_LOG_RETENTION_DAYS;

pub async fn action_history_retention_days(repo: &dyn SettingsRepo) -> Result<u32, StorageError> {
    let raw = repo
        .get_string(reserved_keys::ACTION_HISTORY_RETENTION_DAYS)
        .await?;
    let days = match raw.as_deref().and_then(|s| s.trim().parse().ok()) {
        Some(days) => days,
        None => event_log_retention_days(repo).await?,
    };
    Ok(clamp_action_history_retention_days(days))
}

pub async fn set_action_history_retention_days(
    repo: &dyn SettingsRepo,
    days: u32,
) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::ACTION_HISTORY_RETENTION_DAYS,
        &clamp_action_history_retention_days(days).to_string(),
    )
    .await
}

pub fn clamp_action_history_retention_days(days: u32) -> u32 {
    days.clamp(
        MIN_ACTION_HISTORY_RETENTION_DAYS,
        MAX_ACTION_HISTORY_RETENTION_DAYS,
    )
}

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
