use async_trait::async_trait;
use forge_storage::{StorageError, UserGlobalEntry, UserGlobalsRepo};
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

pub struct SqliteUserGlobalsRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteUserGlobalsRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserGlobalsRepo for SqliteUserGlobalsRepo {
    async fn get(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT value FROM user_globals \
             WHERE broadcaster_id = ? AND user_id = ? AND name = ?",
        )
        .bind(broadcaster_id)
        .bind(user_id)
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

    async fn set(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
        value: Variant,
    ) -> Result<(), StorageError> {
        let value_json = serde_json::to_string(&value).map_err(StorageError::Serialization)?;
        let type_tag = value.type_tag().to_string();
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO user_globals (broadcaster_id, user_id, name, value, type_tag, last_modified) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(broadcaster_id, user_id, name) DO UPDATE SET \
                 value         = excluded.value, \
                 type_tag      = excluded.type_tag, \
                 last_modified = excluded.last_modified",
        )
        .bind(broadcaster_id)
        .bind(user_id)
        .bind(name)
        .bind(&value_json)
        .bind(&type_tag)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "DELETE FROM user_globals \
             WHERE broadcaster_id = ? AND user_id = ? AND name = ?",
        )
        .bind(broadcaster_id)
        .bind(user_id)
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_for_user(
        &self,
        broadcaster_id: &str,
        user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        let rows: Vec<UserGlobalRow> = sqlx::query_as(
            "SELECT broadcaster_id, user_id, name, value, last_modified \
             FROM user_globals \
             WHERE broadcaster_id = ? AND user_id = ?",
        )
        .bind(broadcaster_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }

    async fn list_for_broadcaster(
        &self,
        broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        let rows: Vec<UserGlobalRow> = sqlx::query_as(
            "SELECT broadcaster_id, user_id, name, value, last_modified \
             FROM user_globals \
             WHERE broadcaster_id = ?",
        )
        .bind(broadcaster_id)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }
}

#[derive(sqlx::FromRow)]
struct UserGlobalRow {
    broadcaster_id: String,
    user_id: String,
    name: String,
    value: String,
    last_modified: i64,
}

fn decode_rows(rows: Vec<UserGlobalRow>) -> Result<Vec<UserGlobalEntry>, StorageError> {
    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let value: Variant = serde_json::from_str(&row.value)
            .map_err(|e| StorageError::Parse(format!("variant decode for '{}': {e}", row.name)))?;
        let last_modified = from_epoch_ms(row.last_modified)?;
        entries.push(UserGlobalEntry {
            broadcaster_id: row.broadcaster_id,
            user_id: row.user_id,
            name: row.name,
            value,
            last_modified,
        });
    }
    Ok(entries)
}
