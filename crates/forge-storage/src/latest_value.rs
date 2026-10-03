use async_trait::async_trait;
use serde_json::Value;
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, PartialEq)]
pub struct LatestRecord {
    pub slot: String,
    pub platform: String,
    pub payload: Value,
    pub occurred_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait LatestValueRepo: Send + Sync {
    async fn upsert_if_newer(&self, record: &LatestRecord) -> Result<bool, StorageError>;

    async fn list_slot(&self, slot: &str) -> Result<Vec<LatestRecord>, StorageError>;

    async fn reset_slot(&self, slot: &str) -> Result<u64, StorageError>;
}
