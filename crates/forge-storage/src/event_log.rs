use std::sync::Arc;

use async_trait::async_trait;
use forge_events::Event;
use forge_types::EventId;
use time::OffsetDateTime;

use crate::settings::reserved_keys;
use crate::{SettingsRepo, StorageError};

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait EventLogRepo: Send + Sync {
    async fn insert(&self, event: &Event) -> Result<(), StorageError>;

    /// All-or-nothing; an id already stored is kept as it is. The default writes row by row
    /// and is not atomic, so a persistent backend overrides it.
    async fn insert_batch(&self, events: &[Arc<Event>]) -> Result<(), StorageError> {
        for event in events {
            self.insert(event).await?;
        }
        Ok(())
    }
    async fn get(&self, id: EventId) -> Result<Option<Event>, StorageError>;

    /// Returns up to `limit` events ordered newest-first.
    async fn recent(&self, limit: usize) -> Result<Vec<Event>, StorageError>;

    /// Like `recent`, but only events strictly newer than `since`'s timestamp; an
    /// absent anchor event yields an empty result rather than falling back to `recent`.
    async fn recent_since(
        &self,
        limit: usize,
        since: Option<EventId>,
    ) -> Result<Vec<Event>, StorageError>;

    /// Returns rows deleted.
    async fn prune_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError>;
}

pub const DEFAULT_EVENT_LOG_RETENTION_DAYS: u32 = 7;
pub const MIN_EVENT_LOG_RETENTION_DAYS: u32 = 1;
pub const MAX_EVENT_LOG_RETENTION_DAYS: u32 = 365;

/// Clamped to `MIN_EVENT_LOG_RETENTION_DAYS..=MAX_EVENT_LOG_RETENTION_DAYS`; unset or unparsable reads as the default.
pub async fn event_log_retention_days(repo: &dyn SettingsRepo) -> Result<u32, StorageError> {
    let raw = repo
        .get_string(reserved_keys::EVENT_LOG_RETENTION_DAYS)
        .await?;
    Ok(raw.as_deref().and_then(|s| s.trim().parse().ok()).map_or(
        DEFAULT_EVENT_LOG_RETENTION_DAYS,
        clamp_event_log_retention_days,
    ))
}

/// Stores the clamped value, so a caller never persists a window the pruner would not honour.
pub async fn set_event_log_retention_days(
    repo: &dyn SettingsRepo,
    days: u32,
) -> Result<(), StorageError> {
    repo.set_string(
        reserved_keys::EVENT_LOG_RETENTION_DAYS,
        &clamp_event_log_retention_days(days).to_string(),
    )
    .await
}

pub fn clamp_event_log_retention_days(days: u32) -> u32 {
    days.clamp(MIN_EVENT_LOG_RETENTION_DAYS, MAX_EVENT_LOG_RETENTION_DAYS)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MapRepo(Mutex<HashMap<String, String>>);

    #[async_trait]
    impl SettingsRepo for MapRepo {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(key).is_some())
        }
        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    async fn stored_raw(raw: Option<&str>) -> u32 {
        let repo = MapRepo::default();
        if let Some(raw) = raw {
            repo.set_string(reserved_keys::EVENT_LOG_RETENTION_DAYS, raw)
                .await
                .unwrap();
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
            let repo = MapRepo::default();
            set_event_log_retention_days(&repo, days).await.unwrap();
            stored.push(
                repo.get_string(reserved_keys::EVENT_LOG_RETENTION_DAYS)
                    .await
                    .unwrap(),
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
