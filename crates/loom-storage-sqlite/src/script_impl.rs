use async_trait::async_trait;
use loom_storage::{ScriptRecord, ScriptRepo, StorageError};
use loom_types::ScriptId;
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

fn parse_script_id(s: &str) -> Result<ScriptId, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid script id '{s}': {e}")))
}

fn decode_row(
    id_str: String,
    name: String,
    source_code: String,
    description: Option<String>,
    enabled: i64,
    created_at_ms: i64,
    last_modified_ms: i64,
) -> Result<ScriptRecord, SqliteStorageError> {
    Ok(ScriptRecord {
        id: parse_script_id(&id_str)?,
        name,
        source_code,
        description,
        enabled: enabled != 0,
        created_at: from_epoch_ms(created_at_ms)?,
        last_modified: from_epoch_ms(last_modified_ms)?,
    })
}

pub struct SqliteScriptRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteScriptRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScriptRepo for SqliteScriptRepo {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        let id_str = id.to_string();
        let row: Option<(String, String, String, Option<String>, i64, i64, i64)> = sqlx::query_as(
            "SELECT id, name, source_code, description, enabled, created_at, last_modified \
                 FROM scripts WHERE id = ?",
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
                source_code,
                description,
                enabled,
                created_at_ms,
                last_modified_ms,
            )) => decode_row(
                id_s,
                name,
                source_code,
                description,
                enabled,
                created_at_ms,
                last_modified_ms,
            )
            .map(Some)
            .map_err(StorageError::from),
        }
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        let row: Option<(String, String, String, Option<String>, i64, i64, i64)> = sqlx::query_as(
            "SELECT id, name, source_code, description, enabled, created_at, last_modified \
                 FROM scripts WHERE name = ?",
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
                source_code,
                description,
                enabled,
                created_at_ms,
                last_modified_ms,
            )) => decode_row(
                id_s,
                name,
                source_code,
                description,
                enabled,
                created_at_ms,
                last_modified_ms,
            )
            .map(Some)
            .map_err(StorageError::from),
        }
    }

    async fn upsert(&self, record: ScriptRecord) -> Result<(), StorageError> {
        let id_str = record.id.to_string();
        let now_ms = epoch_ms_now();
        let enabled_int: i64 = if record.enabled { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO scripts (id, name, source_code, description, enabled, created_at, last_modified)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name          = excluded.name,
                 source_code   = excluded.source_code,
                 description   = excluded.description,
                 enabled       = excluded.enabled,
                 last_modified = ?",
        )
        .bind(&id_str)
        .bind(&record.name)
        .bind(&record.source_code)
        .bind(&record.description)
        .bind(enabled_int)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    /// Returns true if a row was actually removed.
    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM scripts WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let rows: Vec<(String, String, String, Option<String>, i64, i64, i64)> = sqlx::query_as(
            "SELECT id, name, source_code, description, enabled, created_at, last_modified \
                 FROM scripts ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let mut records = Vec::with_capacity(rows.len());
        for (id_s, name, source_code, description, enabled, created_at_ms, last_modified_ms) in rows
        {
            let record = decode_row(
                id_s,
                name,
                source_code,
                description,
                enabled,
                created_at_ms,
                last_modified_ms,
            )
            .map_err(StorageError::from)?;
            records.push(record);
        }

        Ok(records)
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let rows: Vec<(String, String, String, Option<String>, i64, i64, i64)> = sqlx::query_as(
            "SELECT id, name, source_code, description, enabled, created_at, last_modified \
                 FROM scripts WHERE enabled = 1 ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let mut records = Vec::with_capacity(rows.len());
        for (id_s, name, source_code, description, enabled, created_at_ms, last_modified_ms) in rows
        {
            let record = decode_row(
                id_s,
                name,
                source_code,
                description,
                enabled,
                created_at_ms,
                last_modified_ms,
            )
            .map_err(StorageError::from)?;
            records.push(record);
        }

        Ok(records)
    }
}
