use async_trait::async_trait;
use loom_storage::{ActionRecord, ActionRepo, StorageError};
use loom_types::ActionId;
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

fn parse_action_id(s: &str) -> Result<ActionId, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid action id '{s}': {e}")))
}

fn decode_row(
    id_str: String,
    name: String,
    config_json: String,
    created_at_ms: i64,
    last_modified_ms: i64,
) -> Result<ActionRecord, SqliteStorageError> {
    let id = parse_action_id(&id_str)?;
    Ok(ActionRecord {
        id,
        name,
        config_json,
        created_at: from_epoch_ms(created_at_ms)?,
        last_modified: from_epoch_ms(last_modified_ms)?,
    })
}

pub struct SqliteActionRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteActionRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ActionRepo for SqliteActionRepo {
    async fn get(&self, id: ActionId) -> Result<Option<ActionRecord>, StorageError> {
        let id_str = id.to_string();
        let row: Option<(String, String, String, i64, i64)> = sqlx::query_as(
            "SELECT id, name, config_json, created_at, last_modified FROM actions WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        match row {
            None => Ok(None),
            Some((id_s, name, config_json, created_at_ms, last_modified_ms)) => {
                decode_row(id_s, name, config_json, created_at_ms, last_modified_ms)
                    .map(Some)
                    .map_err(StorageError::from)
            }
        }
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ActionRecord>, StorageError> {
        let row: Option<(String, String, String, i64, i64)> = sqlx::query_as(
            "SELECT id, name, config_json, created_at, last_modified FROM actions WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        match row {
            None => Ok(None),
            Some((id_s, name, config_json, created_at_ms, last_modified_ms)) => {
                decode_row(id_s, name, config_json, created_at_ms, last_modified_ms)
                    .map(Some)
                    .map_err(StorageError::from)
            }
        }
    }

    async fn upsert(&self, record: ActionRecord) -> Result<(), StorageError> {
        let id_str = record.id.to_string();
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO actions (id, name, config_json, created_at, last_modified)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name          = excluded.name,
                 config_json   = excluded.config_json,
                 last_modified = ?",
        )
        .bind(&id_str)
        .bind(&record.name)
        .bind(&record.config_json)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM actions WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<ActionRecord>, StorageError> {
        let rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as(
            "SELECT id, name, config_json, created_at, last_modified FROM actions ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let mut records = Vec::with_capacity(rows.len());
        for (id_s, name, config_json, created_at_ms, last_modified_ms) in rows {
            let record = decode_row(id_s, name, config_json, created_at_ms, last_modified_ms)
                .map_err(StorageError::from)?;
            records.push(record);
        }

        Ok(records)
    }
}
