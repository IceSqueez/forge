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

pub async fn event_log_retention_days(repo: &dyn SettingsRepo) -> Result<u32, StorageError> {
    let raw = repo
        .get_string(reserved_keys::EVENT_LOG_RETENTION_DAYS)
        .await?;
    Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(7))
}

pub async fn set_event_log_retention_days(
    repo: &dyn SettingsRepo,
    days: u32,
) -> Result<(), StorageError> {
    repo.set_string(reserved_keys::EVENT_LOG_RETENTION_DAYS, &days.to_string())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _check(_: &dyn EventLogRepo) {}
}
