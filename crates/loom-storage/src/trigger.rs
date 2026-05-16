use async_trait::async_trait;
use loom_events::EventSource;
use loom_types::{ActionId, TriggerId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerRecord {
    pub id: TriggerId,
    pub name: String,
    pub source: EventSource,
    pub pattern_json: String,
    pub action_id: ActionId,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
}

#[async_trait]
pub trait TriggerRepo: Send + Sync {
    async fn get(&self, id: TriggerId) -> Result<Option<TriggerRecord>, StorageError>;
    async fn upsert(&self, record: TriggerRecord) -> Result<(), StorageError>;
    /// Returns true if a row was actually removed.
    async fn delete(&self, id: TriggerId) -> Result<bool, StorageError>;
    async fn list(&self) -> Result<Vec<TriggerRecord>, StorageError>;
    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerRecord>, StorageError>;
    async fn list_enabled_by_source(
        &self,
        source: EventSource,
    ) -> Result<Vec<TriggerRecord>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn TriggerRepo) {}

    #[test]
    fn trigger_record_serde_roundtrip() {
        let id = TriggerId::new();
        let action_id = ActionId::new();
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = TriggerRecord {
            id,
            name: "on_cheer".to_owned(),
            source: EventSource::Twitch,
            pattern_json: r#"{"min_bits":100}"#.to_owned(),
            action_id,
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: TriggerRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.name, record.name);
        assert_eq!(decoded.source, record.source);
        assert_eq!(decoded.pattern_json, record.pattern_json);
        assert_eq!(decoded.action_id, record.action_id);
        assert_eq!(decoded.enabled, record.enabled);
        assert_eq!(decoded.created_at, record.created_at);
        assert_eq!(decoded.last_modified, record.last_modified);
    }
}
