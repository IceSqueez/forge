use async_trait::async_trait;
use forge_events::{Event, EventSource};
use forge_storage::{EventLogRepo, StorageError};
use forge_types::EventId;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn to_epoch_secs(dt: OffsetDateTime) -> i64 {
    dt.unix_timestamp()
}

fn from_epoch_secs(secs: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp(secs)
        .map_err(|e| SqliteStorageError::Decode(format!("timestamp {secs} out of range: {e}")))
}

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} '{s}': {e}")))
}

type EventLogRow = (String, String, String, i64, String, Option<String>, i64);

fn decode_row(row: EventLogRow) -> Result<Event, SqliteStorageError> {
    let (id_str, source_str, kind, timestamp_secs, payload_str, caused_by_str, replay_int) = row;

    let id: EventId = parse_id(&id_str, "event id")?;
    let source: EventSource = parse_id(&source_str, "event source")?;
    let timestamp = from_epoch_secs(timestamp_secs)?;
    let payload: serde_json::Value = serde_json::from_str(&payload_str)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid payload json: {e}")))?;
    let caused_by: Option<EventId> = caused_by_str
        .as_deref()
        .map(|s| parse_id(s, "caused_by"))
        .transpose()?;

    Ok(Event {
        id,
        source,
        kind,
        timestamp,
        payload,
        caused_by,
        replay: replay_int != 0,
    })
}

pub struct SqliteEventLogRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteEventLogRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventLogRepo for SqliteEventLogRepo {
    async fn insert(&self, event: &Event) -> Result<(), StorageError> {
        let id = event.id.to_string();
        let source = serde_json::to_string(&event.source)
            .map_err(StorageError::Serialization)?
            .trim_matches('"')
            .to_string();
        let timestamp = to_epoch_secs(event.timestamp);
        let payload = serde_json::to_string(&event.payload).map_err(StorageError::Serialization)?;
        let caused_by = event.caused_by.map(|cid| cid.to_string());
        let replay = i64::from(event.replay);

        sqlx::query(
            "INSERT INTO event_log (id, source, kind, timestamp, payload, caused_by, replay)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&source)
        .bind(&event.kind)
        .bind(timestamp)
        .bind(&payload)
        .bind(caused_by.as_deref())
        .bind(replay)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn get(&self, id: EventId) -> Result<Option<Event>, StorageError> {
        let id_str = id.to_string();
        let row: Option<EventLogRow> = sqlx::query_as(
            "SELECT id, source, kind, timestamp, payload, caused_by, replay
             FROM event_log WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn recent(&self, limit: usize) -> Result<Vec<Event>, StorageError> {
        let rows: Vec<EventLogRow> = sqlx::query_as(
            "SELECT id, source, kind, timestamp, payload, caused_by, replay
             FROM event_log
             ORDER BY timestamp DESC
             LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn prune_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        let cutoff_secs = to_epoch_secs(cutoff);
        let result = sqlx::query("DELETE FROM event_log WHERE timestamp < ?")
            .bind(cutoff_secs)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }
}
