use async_trait::async_trait;
use forge_events::Event;
use forge_storage::{EventLogRepo, StorageError};
use forge_types::EventId;

pub struct NullEventLogRepo;

#[async_trait]
impl EventLogRepo for NullEventLogRepo {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }
}
