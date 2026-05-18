use async_trait::async_trait;
use forge_storage::{GlobalEntry, GlobalsRepo, StorageError};
use forge_types::Variant;
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

pub struct SqliteGlobalsRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteGlobalsRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GlobalsRepo for SqliteGlobalsRepo {
    async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
        let row: Option<(String,)> =
            sqlx::query_as("UPDATE globals SET reads = reads + 1 WHERE name = ? RETURNING value")
                .bind(name)
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        let Some((value_json,)) = row else {
            return Ok(None);
        };

        let variant: Variant = serde_json::from_str(&value_json)
            .map_err(|e| SqliteStorageError::Decode(format!("variant decode: {e}")))?;

        Ok(Some(variant))
    }

    async fn set(&self, name: &str, value: Variant, persisted: bool) -> Result<(), StorageError> {
        let value_json = serde_json::to_string(&value).map_err(StorageError::Serialization)?;
        let type_tag = value.type_tag().to_string();
        let persisted_int: i64 = if persisted { 1 } else { 0 };
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO globals (name, value, type_tag, persisted, reads, writes, created_at, last_modified)
             VALUES (?, ?, ?, ?, 0, 1, ?, ?)
             ON CONFLICT(name) DO UPDATE SET
                 value        = excluded.value,
                 type_tag     = excluded.type_tag,
                 persisted    = excluded.persisted,
                 writes       = writes + 1,
                 last_modified = excluded.last_modified",
        )
        .bind(name)
        .bind(&value_json)
        .bind(&type_tag)
        .bind(persisted_int)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, name: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM globals WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        let rows: Vec<(String, String, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT name, value, persisted, reads, writes, created_at, last_modified FROM globals",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let mut entries = Vec::with_capacity(rows.len());
        for (name, value_json, persisted_int, reads, writes, created_at_ms, last_modified_ms) in
            rows
        {
            let value: Variant = serde_json::from_str(&value_json)
                .map_err(|e| StorageError::Parse(format!("variant decode for '{name}': {e}")))?;

            let created_at = from_epoch_ms(created_at_ms)?;
            let last_modified = from_epoch_ms(last_modified_ms)?;

            entries.push(GlobalEntry {
                name,
                value,
                persisted: persisted_int != 0,
                reads: reads as u64,
                writes: writes as u64,
                created_at,
                last_modified,
            });
        }

        Ok(entries)
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        let bytes: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(LENGTH(name) + LENGTH(value)), 0) FROM globals",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(bytes as u64)
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        let ms: Option<i64> =
            sqlx::query_scalar("SELECT MAX(last_modified) FROM globals WHERE persisted = 1")
                .fetch_one(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        match ms {
            None => Ok(None),
            Some(ms) => from_epoch_ms(ms).map(Some).map_err(StorageError::from),
        }
    }

    async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError> {
        let now_ms = epoch_ms_now();

        let row: Option<(String,)> = sqlx::query_as(
            "UPDATE globals \
             SET value         = json_set(value, '$.value', json_extract(value, '$.value') + ?), \
                 writes        = writes + 1, \
                 last_modified = ? \
             WHERE name = ? AND type_tag IN ('int', 'float') \
             RETURNING value",
        )
        .bind(amount)
        .bind(now_ms)
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        if let Some((value_json,)) = row {
            let variant: Variant = serde_json::from_str(&value_json)
                .map_err(|e| StorageError::Parse(format!("variant decode: {e}")))?;
            return Ok(variant);
        }

        let tag: Option<String> = sqlx::query_scalar("SELECT type_tag FROM globals WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        match tag {
            None => Err(StorageError::NotFound {
                key: name.to_string(),
            }),
            Some(actual) => Err(StorageError::TypeMismatch {
                name: name.to_string(),
                actual,
            }),
        }
    }
}
