use async_trait::async_trait;
use loom_types::ScriptId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptRecord {
    pub id: ScriptId,
    pub name: String,
    pub source_code: String,
    pub description: Option<String>,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub last_modified: OffsetDateTime,
}

#[async_trait]
pub trait ScriptRepo: Send + Sync {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError>;
    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError>;
    async fn upsert(&self, record: ScriptRecord) -> Result<(), StorageError>;
    /// Returns true if a row was actually removed.
    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError>;
    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError>;
    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn ScriptRepo) {}

    #[test]
    fn script_record_serde_roundtrip_with_description() {
        let id = ScriptId::new();
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = ScriptRecord {
            id,
            name: "greet_chat".to_owned(),
            source_code: r#"print("hello");"#.to_owned(),
            description: Some("Greets the chat on event.".to_owned()),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: ScriptRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.name, record.name);
        assert_eq!(decoded.source_code, record.source_code);
        assert_eq!(decoded.description, record.description);
        assert_eq!(decoded.enabled, record.enabled);
        assert_eq!(decoded.created_at, record.created_at);
        assert_eq!(decoded.last_modified, record.last_modified);
    }

    #[test]
    fn script_record_serde_roundtrip_without_description() {
        let id = ScriptId::new();
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = ScriptRecord {
            id,
            name: "silent_hook".to_owned(),
            source_code: r#"let x = 1;"#.to_owned(),
            description: None,
            enabled: false,
            created_at: ts,
            last_modified: ts,
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: ScriptRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.name, record.name);
        assert_eq!(decoded.source_code, record.source_code);
        assert_eq!(decoded.description, None);
        assert_eq!(decoded.enabled, record.enabled);
        assert_eq!(decoded.created_at, record.created_at);
        assert_eq!(decoded.last_modified, record.last_modified);
    }
}
