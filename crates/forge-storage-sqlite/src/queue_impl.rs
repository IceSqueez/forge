use async_trait::async_trait;
use forge_storage::{QueueRecord, QueueRepo, StorageError};
use forge_types::QueueId;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn epoch_ms_now() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}

fn parse_queue_id(s: &str) -> Result<QueueId, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid queue id '{s}': {e}")))
}

fn decode_row(
    id_str: String,
    name: String,
    blocking_int: i64,
    enabled_int: i64,
    paused_int: i64,
    created_at_ms: i64,
    last_modified_ms: i64,
) -> Result<QueueRecord, SqliteStorageError> {
    let id = parse_queue_id(&id_str)?;

    Ok(QueueRecord {
        id,
        name,
        blocking: blocking_int != 0,
        enabled: enabled_int != 0,
        paused: paused_int != 0,
        created_at: from_epoch_ms(created_at_ms)?,
        last_modified: from_epoch_ms(last_modified_ms)?,
    })
}

pub struct SqliteQueueRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteQueueRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QueueRepo for SqliteQueueRepo {
    async fn get(&self, id: QueueId) -> Result<Option<QueueRecord>, StorageError> {
        let id_str = id.to_string();
        let row: Option<(String, String, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT id, name, blocking, enabled, paused, created_at, last_modified
             FROM queues WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        match row {
            None => Ok(None),
            Some((
                id_s,
                name,
                blocking_int,
                enabled_int,
                paused_int,
                created_at_ms,
                last_modified_ms,
            )) => decode_row(
                id_s,
                name,
                blocking_int,
                enabled_int,
                paused_int,
                created_at_ms,
                last_modified_ms,
            )
            .map(Some)
            .map_err(StorageError::from),
        }
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<QueueRecord>, StorageError> {
        let row: Option<(String, String, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT id, name, blocking, enabled, paused, created_at, last_modified
             FROM queues WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        match row {
            None => Ok(None),
            Some((
                id_s,
                name,
                blocking_int,
                enabled_int,
                paused_int,
                created_at_ms,
                last_modified_ms,
            )) => decode_row(
                id_s,
                name,
                blocking_int,
                enabled_int,
                paused_int,
                created_at_ms,
                last_modified_ms,
            )
            .map(Some)
            .map_err(StorageError::from),
        }
    }

    async fn upsert(&self, record: QueueRecord) -> Result<(), StorageError> {
        let id_str = record.id.to_string();
        let blocking_int: i64 = if record.blocking { 1 } else { 0 };
        let enabled_int: i64 = if record.enabled { 1 } else { 0 };
        let paused_int: i64 = if record.paused { 1 } else { 0 };
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO queues (id, name, blocking, enabled, paused, created_at, last_modified)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name          = excluded.name,
                 blocking      = excluded.blocking,
                 enabled       = excluded.enabled,
                 paused        = excluded.paused,
                 last_modified = ?",
        )
        .bind(&id_str)
        .bind(&record.name)
        .bind(blocking_int)
        .bind(enabled_int)
        .bind(paused_int)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: QueueId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM queues WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<QueueRecord>, StorageError> {
        let rows: Vec<(String, String, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT id, name, blocking, enabled, paused, created_at, last_modified
             FROM queues ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(
                |(
                    id_s,
                    name,
                    blocking_int,
                    enabled_int,
                    paused_int,
                    created_at_ms,
                    last_modified_ms,
                )| {
                    decode_row(
                        id_s,
                        name,
                        blocking_int,
                        enabled_int,
                        paused_int,
                        created_at_ms,
                        last_modified_ms,
                    )
                    .map_err(StorageError::from)
                },
            )
            .collect()
    }

    async fn set_paused(&self, id: QueueId, paused: bool) -> Result<(), StorageError> {
        let id_str = id.to_string();
        let paused_int: i64 = if paused { 1 } else { 0 };
        let now_ms = epoch_ms_now();

        sqlx::query("UPDATE queues SET paused = ?, last_modified = ? WHERE id = ?")
            .bind(paused_int)
            .bind(now_ms)
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }
}
