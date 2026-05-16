use async_trait::async_trait;
use loom_types::{ActionId, CommandId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRecord {
    pub id: CommandId,
    pub name: String,
    pub action_id: ActionId,
    pub cooldown_ms: u32,
    pub permission: String,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
}

#[async_trait]
pub trait CommandRepo: Send + Sync {
    async fn get(&self, id: CommandId) -> Result<Option<CommandRecord>, StorageError>;
    async fn get_by_name(&self, name: &str) -> Result<Option<CommandRecord>, StorageError>;
    async fn upsert(&self, record: CommandRecord) -> Result<(), StorageError>;
    /// Returns true if a row was actually removed.
    async fn delete(&self, id: CommandId) -> Result<bool, StorageError>;
    async fn list(&self) -> Result<Vec<CommandRecord>, StorageError>;
    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<CommandRecord>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn CommandRepo) {}

    #[test]
    fn command_record_serde_roundtrip() {
        let id = CommandId::new();
        let action_id = ActionId::new();
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = CommandRecord {
            id,
            name: "!hello".to_owned(),
            action_id,
            cooldown_ms: 5000,
            permission: "viewer".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: CommandRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.name, record.name);
        assert_eq!(decoded.action_id, record.action_id);
        assert_eq!(decoded.cooldown_ms, record.cooldown_ms);
        assert_eq!(decoded.permission, record.permission);
        assert_eq!(decoded.enabled, record.enabled);
        assert_eq!(decoded.created_at, record.created_at);
        assert_eq!(decoded.last_modified, record.last_modified);
    }
}
