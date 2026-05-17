use async_trait::async_trait;
use forge_types::QueueId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueRecord {
    pub id: QueueId,
    pub name: String,
    pub blocking: bool,
    pub enabled: bool,
    pub paused: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
}

#[async_trait]
pub trait QueueRepo: Send + Sync {
    async fn get(&self, id: QueueId) -> Result<Option<QueueRecord>, StorageError>;
    async fn get_by_name(&self, name: &str) -> Result<Option<QueueRecord>, StorageError>;
    async fn upsert(&self, record: QueueRecord) -> Result<(), StorageError>;
    /// Returns true if a row was actually removed.
    async fn delete(&self, id: QueueId) -> Result<bool, StorageError>;
    async fn list(&self) -> Result<Vec<QueueRecord>, StorageError>;
    async fn set_paused(&self, id: QueueId, paused: bool) -> Result<(), StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn QueueRepo) {}

    #[test]
    fn queue_record_serde_roundtrip() {
        let id = QueueId::new();
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = QueueRecord {
            id,
            name: "default".to_owned(),
            blocking: false,
            enabled: true,
            paused: false,
            created_at: ts,
            last_modified: ts,
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: QueueRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.name, record.name);
        assert_eq!(decoded.blocking, record.blocking);
        assert_eq!(decoded.enabled, record.enabled);
        assert_eq!(decoded.paused, record.paused);
        assert_eq!(decoded.created_at, record.created_at);
        assert_eq!(decoded.last_modified, record.last_modified);
    }
}
