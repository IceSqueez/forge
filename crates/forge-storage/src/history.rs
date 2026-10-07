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
    use crate::event_log::tests::MapRepo;
    use crate::set_event_log_retention_days;

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

    async fn read_back(history_raw: Option<&str>, event_log_days: Option<u32>) -> u32 {
        let repo = MapRepo::default();
        if let Some(raw) = history_raw {
            repo.set_string(reserved_keys::ACTION_HISTORY_RETENTION_DAYS, raw)
                .await
                .unwrap();
        }
        if let Some(days) = event_log_days {
            set_event_log_retention_days(&repo, days).await.unwrap();
        }
        action_history_retention_days(&repo).await.unwrap()
    }

    #[tokio::test]
    async fn a_stored_action_history_retention_reads_clamped_to_seven_through_365_days() {
        for (raw, expected) in [
            ("0", 7),
            ("6", 7),
            ("7", 7),
            ("8", 8),
            ("364", 364),
            ("365", 365),
            ("366", 365),
            ("4294967295", 365),
            (" 30 ", 30),
        ] {
            assert_eq!(
                read_back(Some(raw), Some(1)).await,
                expected,
                "stored {raw:?}"
            );
        }
    }

    #[tokio::test]
    async fn an_unset_or_unparsable_action_history_retention_follows_the_event_log_retention_clamped()
     {
        for (raw, event_log_days, expected) in [
            (None, None, 7),
            (None, Some(3), 7),
            (None, Some(30), 30),
            (None, Some(365), 365),
            (Some(""), Some(30), 30),
            (Some("abc"), Some(30), 30),
            (Some("-5"), Some(3), 7),
            (Some("1.5"), Some(30), 30),
        ] {
            assert_eq!(
                read_back(raw, event_log_days).await,
                expected,
                "stored {raw:?}, event log {event_log_days:?}"
            );
        }
    }

    #[tokio::test]
    async fn setting_the_action_history_retention_stores_the_clamped_value() {
        let mut stored = Vec::new();
        for days in [0, 6, 7, 365, 366] {
            let repo = MapRepo::default();
            set_action_history_retention_days(&repo, days)
                .await
                .unwrap();
            stored.push(
                repo.get_string(reserved_keys::ACTION_HISTORY_RETENTION_DAYS)
                    .await
                    .unwrap(),
            );
        }

        assert_eq!(
            stored,
            ["7", "7", "7", "365", "365"]
                .map(|v| Some(v.to_owned()))
                .to_vec()
        );
    }
}
