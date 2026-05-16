use async_trait::async_trait;
use loom_storage::{HistoryOutcome, HistoryRecord, HistoryRepo, NewHistoryRecord, StorageError};
use loom_types::{ActionId, EventId};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / 1_000_000) as i64
}

fn parse_action_id(s: &str) -> Result<ActionId, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid action id '{s}': {e}")))
}

fn parse_event_id(s: &str) -> Result<EventId, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid event id '{s}': {e}")))
}

fn decode_row(
    id: i64,
    action_id_str: String,
    triggering_event_id_str: Option<String>,
    started_at_ms: i64,
    duration_ms: i64,
    outcome_str: String,
    context_json: String,
) -> Result<HistoryRecord, SqliteStorageError> {
    let action_id = parse_action_id(&action_id_str)?;
    let triggering_event_id = triggering_event_id_str
        .as_deref()
        .map(parse_event_id)
        .transpose()?;
    let started_at = from_epoch_ms(started_at_ms)?;
    let outcome = outcome_str
        .parse::<HistoryOutcome>()
        .map_err(|e| SqliteStorageError::Decode(e.to_string()))?;

    Ok(HistoryRecord {
        id,
        action_id,
        triggering_event_id,
        started_at,
        duration_ms: duration_ms as u64,
        outcome,
        context_json,
    })
}

type HistoryRow = (i64, String, Option<String>, i64, i64, String, String);

pub struct SqliteHistoryRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteHistoryRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HistoryRepo for SqliteHistoryRepo {
    async fn record(&self, new: NewHistoryRecord) -> Result<i64, StorageError> {
        let action_id_str = new.action_id.to_string();
        let triggering_event_id_str = new.triggering_event_id.map(|e| e.to_string());
        let started_at_ms = to_epoch_ms(new.started_at);
        let duration_ms = new.duration_ms as i64;
        let outcome_str = new.outcome.to_string();

        let result = sqlx::query(
            "INSERT INTO action_history
                (action_id, triggering_event_id, started_at, duration_ms, outcome, context)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&action_id_str)
        .bind(&triggering_event_id_str)
        .bind(started_at_ms)
        .bind(duration_ms)
        .bind(&outcome_str)
        .bind(&new.context_json)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.last_insert_rowid())
    }

    async fn get(&self, id: i64) -> Result<Option<HistoryRecord>, StorageError> {
        let row: Option<HistoryRow> = sqlx::query_as(
            "SELECT id, action_id, triggering_event_id, started_at, duration_ms, outcome, context
                 FROM action_history WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        match row {
            None => Ok(None),
            Some((id, aid, eid, sat, dur, out, ctx)) => {
                decode_row(id, aid, eid, sat, dur, out, ctx)
                    .map(Some)
                    .map_err(StorageError::from)
            }
        }
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
        limit: u32,
    ) -> Result<Vec<HistoryRecord>, StorageError> {
        let action_id_str = action_id.to_string();
        let rows: Vec<HistoryRow> = sqlx::query_as(
            "SELECT id, action_id, triggering_event_id, started_at, duration_ms, outcome, context
                 FROM action_history
                 WHERE action_id = ?
                 ORDER BY started_at DESC
                 LIMIT ?",
        )
        .bind(&action_id_str)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }

    async fn list_recent(&self, limit: u32) -> Result<Vec<HistoryRecord>, StorageError> {
        let rows: Vec<HistoryRow> = sqlx::query_as(
            "SELECT id, action_id, triggering_event_id, started_at, duration_ms, outcome, context
                 FROM action_history
                 ORDER BY started_at DESC
                 LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }

    async fn list_caused_by(&self, event_id: EventId) -> Result<Vec<HistoryRecord>, StorageError> {
        let event_id_str = event_id.to_string();
        let rows: Vec<HistoryRow> = sqlx::query_as(
            "SELECT id, action_id, triggering_event_id, started_at, duration_ms, outcome, context
                 FROM action_history
                 WHERE triggering_event_id = ?",
        )
        .bind(&event_id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }

    async fn prune_older_than(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        let cutoff_ms = to_epoch_ms(cutoff);
        let result = sqlx::query("DELETE FROM action_history WHERE started_at < ?")
            .bind(cutoff_ms)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }
}

fn decode_rows(rows: Vec<HistoryRow>) -> Result<Vec<HistoryRecord>, StorageError> {
    let mut records = Vec::with_capacity(rows.len());
    for (id, aid, eid, sat, dur, out, ctx) in rows {
        let record = decode_row(id, aid, eid, sat, dur, out, ctx).map_err(StorageError::from)?;
        records.push(record);
    }
    Ok(records)
}
