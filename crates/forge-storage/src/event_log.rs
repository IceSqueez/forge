use async_trait::async_trait;
use forge_events::Event;
use forge_types::EventId;
use time::OffsetDateTime;

use crate::StorageError;

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait EventLogRepo: Send + Sync {
    async fn insert(&self, event: &Event) -> Result<(), StorageError>;
    async fn get(&self, id: EventId) -> Result<Option<Event>, StorageError>;

    /// Returns up to `limit` events ordered newest-first.
    async fn recent(&self, limit: usize) -> Result<Vec<Event>, StorageError>;

    /// Returns up to `limit` events ordered newest-first.
    ///
    /// When `since` is `Some(id)`, only events whose timestamp is strictly
    /// greater than the timestamp of the event identified by `id` are returned.
    /// If the anchor event is absent from the store the result is empty.
    /// When `since` is `None` this is equivalent to `recent(limit)`.
    async fn recent_since(
        &self,
        limit: usize,
        since: Option<EventId>,
    ) -> Result<Vec<Event>, StorageError>;

    /// Deletes all events whose timestamp is strictly before `cutoff`.
    /// Returns the number of rows deleted.
    async fn prune_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _check(_: &dyn EventLogRepo) {}
}
