use async_trait::async_trait;
use loom_events::EventSource;
use loom_storage::{StorageError, TriggerRecord, TriggerRepo};
use loom_types::{ActionId, TriggerId};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

type TriggerRow = (String, String, String, String, String, i64, i64, i64);

fn epoch_ms_now() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}

fn parse_id<T>(s: &str, label: &str) -> Result<T, SqliteStorageError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} id '{s}': {e}")))
}

fn decode_row(row: TriggerRow) -> Result<TriggerRecord, SqliteStorageError> {
    let (
        id_str,
        name,
        source_json,
        pattern_json,
        action_id_str,
        enabled_int,
        created_at_ms,
        last_modified_ms,
    ) = row;

    let id: TriggerId = parse_id(&id_str, "trigger")?;
    let source: EventSource = serde_json::from_str(&source_json).map_err(|e| {
        SqliteStorageError::Decode(format!("invalid event source '{source_json}': {e}"))
    })?;
    let action_id: ActionId = parse_id(&action_id_str, "action")?;

    Ok(TriggerRecord {
        id,
        name,
        source,
        pattern_json,
        action_id,
        enabled: enabled_int != 0,
        created_at: from_epoch_ms(created_at_ms)?,
        last_modified: from_epoch_ms(last_modified_ms)?,
    })
}

pub struct SqliteTriggerRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteTriggerRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TriggerRepo for SqliteTriggerRepo {
    async fn get(&self, id: TriggerId) -> Result<Option<TriggerRecord>, StorageError> {
        let id_str = id.to_string();
        let row: Option<TriggerRow> = sqlx::query_as(
            "SELECT id, name, source, pattern_json, action_id, enabled, created_at, last_modified
             FROM triggers WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn upsert(&self, record: TriggerRecord) -> Result<(), StorageError> {
        let id_str = record.id.to_string();
        let source_json =
            serde_json::to_string(&record.source).map_err(StorageError::Serialization)?;
        let action_id_str = record.action_id.to_string();
        let enabled_int: i64 = if record.enabled { 1 } else { 0 };
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO triggers (id, name, source, pattern_json, action_id, enabled, created_at, last_modified)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name          = excluded.name,
                 source        = excluded.source,
                 pattern_json  = excluded.pattern_json,
                 action_id     = excluded.action_id,
                 enabled       = excluded.enabled,
                 last_modified = ?",
        )
        .bind(&id_str)
        .bind(&record.name)
        .bind(&source_json)
        .bind(&record.pattern_json)
        .bind(&action_id_str)
        .bind(enabled_int)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: TriggerId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM triggers WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<TriggerRecord>, StorageError> {
        let rows: Vec<TriggerRow> = sqlx::query_as(
            "SELECT id, name, source, pattern_json, action_id, enabled, created_at, last_modified
             FROM triggers ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerRecord>, StorageError> {
        let action_id_str = action_id.to_string();
        let rows: Vec<TriggerRow> = sqlx::query_as(
            "SELECT id, name, source, pattern_json, action_id, enabled, created_at, last_modified
             FROM triggers WHERE action_id = ? ORDER BY name",
        )
        .bind(&action_id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn list_enabled_by_source(
        &self,
        source: EventSource,
    ) -> Result<Vec<TriggerRecord>, StorageError> {
        let source_json = serde_json::to_string(&source).map_err(StorageError::Serialization)?;
        let rows: Vec<TriggerRow> = sqlx::query_as(
            "SELECT id, name, source, pattern_json, action_id, enabled, created_at, last_modified
             FROM triggers WHERE source = ? AND enabled = 1 ORDER BY name",
        )
        .bind(&source_json)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }
}
