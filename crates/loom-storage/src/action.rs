use async_trait::async_trait;
use loom_types::ActionId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub id: ActionId,
    pub name: String,
    pub config_json: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
}

#[async_trait]
pub trait ActionRepo: Send + Sync {
    async fn get(&self, id: ActionId) -> Result<Option<ActionRecord>, StorageError>;
    async fn get_by_name(&self, name: &str) -> Result<Option<ActionRecord>, StorageError>;
    async fn upsert(&self, record: ActionRecord) -> Result<(), StorageError>;
    /// Returns true if a row was actually removed.
    async fn delete(&self, id: ActionId) -> Result<bool, StorageError>;
    async fn list(&self) -> Result<Vec<ActionRecord>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn ActionRepo) {}

    #[test]
    fn action_record_serde_roundtrip() {
        let id = ActionId::new();
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = ActionRecord {
            id,
            name: "my_action".to_owned(),
            config_json: r#"{"sub_actions":[]}"#.to_owned(),
            created_at: ts,
            last_modified: ts,
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: ActionRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.name, record.name);
        assert_eq!(decoded.config_json, record.config_json);
        assert_eq!(decoded.created_at, record.created_at);
        assert_eq!(decoded.last_modified, record.last_modified);
    }
}
