use async_trait::async_trait;
use forge_events::Event;
use forge_storage::{EventLogRepo, StorageError};
use forge_types::EventId;
use time::OffsetDateTime;

pub struct SqliteEventLogRepo;

impl SqliteEventLogRepo {
    pub fn new(_pool: sqlx::SqlitePool) -> Self {
        Self
    }
}

#[async_trait]
impl EventLogRepo for SqliteEventLogRepo {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Err(StorageError::NotReady)
    }

    async fn get(&self, _id: EventId) -> Result<Option<Event>, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Err(StorageError::NotReady)
    }

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Err(StorageError::NotReady)
    }
}
