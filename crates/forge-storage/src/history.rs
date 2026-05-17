use std::str::FromStr;

use async_trait::async_trait;
use forge_types::{ActionId, EventId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryOutcome {
    Ok,
    Err,
}

impl std::fmt::Display for HistoryOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ok => f.write_str("ok"),
            Self::Err => f.write_str("err"),
        }
    }
}

impl FromStr for HistoryOutcome {
    type Err = StorageError;

    fn from_str(s: &str) -> Result<Self, StorageError> {
        match s {
            "ok" => Ok(Self::Ok),
            "err" => Ok(Self::Err),
            other => Err(StorageError::Parse(format!(
                "unknown HistoryOutcome: {other:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub id: i64,
    pub action_id: ActionId,
    pub triggering_event_id: Option<EventId>,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    pub duration_ms: u64,
    pub outcome: HistoryOutcome,
    pub context_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewHistoryRecord {
    pub action_id: ActionId,
    pub triggering_event_id: Option<EventId>,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    pub duration_ms: u64,
    pub outcome: HistoryOutcome,
    pub context_json: String,
}

#[async_trait]
pub trait HistoryRepo: Send + Sync {
    async fn record(&self, new: NewHistoryRecord) -> Result<i64, StorageError>;

    async fn get(&self, id: i64) -> Result<Option<HistoryRecord>, StorageError>;

    async fn list_for_action(
        &self,
        action_id: ActionId,
        limit: u32,
    ) -> Result<Vec<HistoryRecord>, StorageError>;

    async fn list_recent(&self, limit: u32) -> Result<Vec<HistoryRecord>, StorageError>;

    async fn list_caused_by(&self, event_id: EventId) -> Result<Vec<HistoryRecord>, StorageError>;

    /// Returns the number of rows deleted.
    async fn prune_older_than(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn history_repo_is_dyn_safe() {
        fn accepts_repo(_: &dyn HistoryRepo) {}
        let _ = accepts_repo;
    }

    #[test]
    fn history_record_serde_with_event_id() {
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let action_id = ActionId::new();
        let event_id = EventId::new();

        let record = HistoryRecord {
            id: 42,
            action_id,
            triggering_event_id: Some(event_id),
            started_at: ts,
            duration_ms: 123,
            outcome: HistoryOutcome::Ok,
            context_json: r#"{"step_count":3}"#.to_owned(),
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: HistoryRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.id, record.id);
        assert_eq!(decoded.action_id, record.action_id);
        assert_eq!(decoded.triggering_event_id, record.triggering_event_id);
        assert_eq!(decoded.started_at, ts);
        assert_eq!(decoded.duration_ms, 123);
        assert_eq!(decoded.outcome, HistoryOutcome::Ok);
        assert_eq!(decoded.context_json, record.context_json);
    }

    #[test]
    fn history_record_serde_without_event_id() {
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_001).unwrap();
        let record = HistoryRecord {
            id: 1,
            action_id: ActionId::new(),
            triggering_event_id: None,
            started_at: ts,
            duration_ms: 0,
            outcome: HistoryOutcome::Err,
            context_json: "{}".to_owned(),
        };

        let json = serde_json::to_string(&record).unwrap();
        let decoded: HistoryRecord = serde_json::from_str(&json).unwrap();

        assert!(decoded.triggering_event_id.is_none());
        assert_eq!(decoded.outcome, HistoryOutcome::Err);
    }

    #[test]
    fn history_outcome_serde_lowercase() {
        let ok = serde_json::to_string(&HistoryOutcome::Ok).unwrap();
        let err = serde_json::to_string(&HistoryOutcome::Err).unwrap();
        assert_eq!(ok, r#""ok""#);
        assert_eq!(err, r#""err""#);
    }

    #[test]
    fn history_outcome_from_str_roundtrip() {
        assert_eq!("ok".parse::<HistoryOutcome>().unwrap(), HistoryOutcome::Ok);
        assert_eq!(
            "err".parse::<HistoryOutcome>().unwrap(),
            HistoryOutcome::Err
        );
    }

    #[test]
    fn history_outcome_from_str_unknown_input() {
        let result = "unknown".parse::<HistoryOutcome>();
        assert!(result.is_err());
    }

    #[test]
    fn new_history_record_serde_roundtrip() {
        let ts = OffsetDateTime::from_unix_timestamp(1_700_000_002).unwrap();
        let action_id = ActionId::new();

        let new_record = NewHistoryRecord {
            action_id,
            triggering_event_id: None,
            started_at: ts,
            duration_ms: 55,
            outcome: HistoryOutcome::Ok,
            context_json: r#"{"ok":true}"#.to_owned(),
        };

        let json = serde_json::to_string(&new_record).unwrap();
        let decoded: NewHistoryRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.action_id, action_id);
        assert_eq!(decoded.duration_ms, 55);
        assert_eq!(decoded.outcome, HistoryOutcome::Ok);
    }
}
