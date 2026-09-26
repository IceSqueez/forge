use async_trait::async_trait;
use forge_storage::user_globals::incremented;
use forge_storage::{StorageError, UserGlobalEntry, UserGlobalsRepo};
use forge_types::Variant;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

fn epoch_ms_now() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}

pub struct SqliteUserGlobalsRepo {
    db: SqlitePools,
}

impl SqliteUserGlobalsRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

async fn upsert<'e, E>(
    executor: E,
    broadcaster_id: &str,
    user_id: &str,
    name: &str,
    value: &Variant,
) -> Result<(), StorageError>
where
    E: sqlx::SqliteExecutor<'e>,
{
    let value_json = serde_json::to_string(value).map_err(StorageError::Serialization)?;
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
    .execute(executor)
    .await
    .map_err(SqliteStorageError::Sqlx)?;

    Ok(())
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
        .fetch_optional(self.db.reader())
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
        upsert(self.db.writer(), broadcaster_id, user_id, name, &value).await
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
        .execute(self.db.writer())
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
        .fetch_all(self.db.reader())
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
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_rows(rows)
    }

    async fn incr(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
        amount: i64,
    ) -> Result<Variant, StorageError> {
        let mut tx = self
            .db
            .writer()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        let current: Option<(String,)> = sqlx::query_as(
            "SELECT value FROM user_globals \
             WHERE broadcaster_id = ? AND user_id = ? AND name = ?",
        )
        .bind(broadcaster_id)
        .bind(user_id)
        .bind(name)
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let current = current
            .map(|(value_json,)| {
                serde_json::from_str::<Variant>(&value_json)
                    .map_err(|e| SqliteStorageError::Decode(format!("variant decode: {e}")))
            })
            .transpose()?;
        let next = incremented(name, current, amount)?;
        upsert(&mut *tx, broadcaster_id, user_id, name, &next).await?;

        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(next)
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
