use async_trait::async_trait;
use forge_types::{ScriptContract, ScriptId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptRecord {
    pub id: ScriptId,
    pub name: String,
    pub body: String,
    pub contract: ScriptContract,
    pub body_hash: String,
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
    async fn save(&self, record: ScriptRecord) -> Result<(), StorageError>;
    /// Returns true if a row was removed.
    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError>;
    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError>;
    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_types::{ScriptInput, VariantKind};

    fn _trait_is_dyn_safe(_: &dyn ScriptRepo) {}

    fn make_record(name: &str) -> ScriptRecord {
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        ScriptRecord {
            id: ScriptId::new(),
            name: name.to_owned(),
            body: r#"print("hello");"#.to_owned(),
            contract: ScriptContract::default(),
            body_hash: "abc123".to_owned(),
            enabled: true,
            created_at: ts,
            last_modified: ts,
        }
    }

    #[test]
    fn script_record_serde_roundtrip_empty_contract() {
        let record = make_record("greet_chat");
        let json = serde_json::to_string(&record).unwrap();
        let decoded: ScriptRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.name, record.name);
        assert_eq!(decoded.body, record.body);
        assert_eq!(decoded.contract, record.contract);
        assert_eq!(decoded.body_hash, record.body_hash);
        assert_eq!(decoded.enabled, record.enabled);
        assert_eq!(decoded.created_at, record.created_at);
        assert_eq!(decoded.last_modified, record.last_modified);
    }

    #[test]
    fn script_record_serde_roundtrip_with_contract() {
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let record = ScriptRecord {
            id: ScriptId::new(),
            name: "greet_user".to_owned(),
            body: r#"let msg = "hi " + user;"#.to_owned(),
            contract: ScriptContract {
                inputs: vec![ScriptInput {
                    name: "user".to_owned(),
                    kind: VariantKind::String,
                }],
                returns: Some(VariantKind::String),
            },
            body_hash: "deadbeef".to_owned(),
            enabled: false,
            created_at: ts,
            last_modified: ts,
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: ScriptRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.contract, record.contract);
        assert_eq!(decoded.enabled, record.enabled);
    }
}
