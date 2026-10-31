use std::sync::Arc;

use async_trait::async_trait;
use forge_events::Event;
use forge_types::EventId;

use crate::settings::core_settings;
use crate::settings::typed::{read_setting, write_setting};
use crate::{SettingsRepo, StorageError};

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait EventLogRepo: Send + Sync {
    async fn insert(&self, event: &Event) -> Result<(), StorageError>;

    async fn insert_batch(&self, events: &[Arc<Event>]) -> Result<(), StorageError> {
        for event in events {
            self.insert(event).await?;
        }
        Ok(())
    }
    async fn get(&self, id: EventId) -> Result<Option<Event>, StorageError>;

    async fn recent(&self, limit: usize) -> Result<Vec<Event>, StorageError>;

    async fn recent_since(
        &self,
        limit: usize,
        since: Option<EventId>,
    ) -> Result<Vec<Event>, StorageError>;
}

pub const DEFAULT_EVENT_LOG_RETENTION_DAYS: u32 = 7;
pub const MIN_EVENT_LOG_RETENTION_DAYS: u32 = 1;
pub const MAX_EVENT_LOG_RETENTION_DAYS: u32 = 365;

pub async fn event_log_retention_days(repo: &dyn SettingsRepo) -> Result<u32, StorageError> {
    read_setting(repo, &core_settings::EVENT_LOG_RETENTION_DAYS).await
}

pub async fn set_event_log_retention_days(
    repo: &dyn SettingsRepo,
    days: u32,
) -> Result<(), StorageError> {
    write_setting(repo, &core_settings::EVENT_LOG_RETENTION_DAYS, &days).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::core_settings::EVENT_LOG_RETENTION_DAYS;
    use crate::test_support::MemorySettings;

    async fn stored_raw(raw: Option<&str>) -> u32 {
        let repo = MemorySettings::new();
        if let Some(raw) = raw {
            repo.seed_raw(&EVENT_LOG_RETENTION_DAYS, raw);
        }
        event_log_retention_days(&repo).await.unwrap()
    }

    #[tokio::test]
    async fn reading_the_retention_clamps_stored_values_to_one_through_365_days() {
        for (raw, expected) in [
            ("0", 1),
            ("1", 1),
            ("2", 2),
            ("364", 364),
            ("365", 365),
            ("366", 365),
            ("4294967295", 365),
            (" 30 ", 30),
        ] {
            assert_eq!(stored_raw(Some(raw)).await, expected, "stored {raw:?}");
        }
    }

    #[tokio::test]
    async fn an_unset_or_unparsable_retention_reads_as_seven_days() {
        for raw in [None, Some(""), Some("abc"), Some("-5"), Some("1.5")] {
            assert_eq!(stored_raw(raw).await, 7, "stored {raw:?}");
        }
    }

    #[tokio::test]
    async fn setting_the_retention_stores_the_clamped_value() {
        let mut stored = Vec::new();
        for days in [0, 1, 365, 366] {
            let repo = MemorySettings::new();
            set_event_log_retention_days(&repo, days).await.unwrap();
            stored.push(
                repo.row(EVENT_LOG_RETENTION_DAYS.name())
                    .map(|row| row.value),
            );
        }

        assert_eq!(
            stored,
            ["1", "1", "365", "365"]
                .map(|v| Some(v.to_owned()))
                .to_vec()
        );
    }
}
