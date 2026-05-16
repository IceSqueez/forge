use async_trait::async_trait;
use loom_storage::{CommandRecord, CommandRepo, StorageError};
use loom_types::{ActionId, CommandId};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

type CommandRow = (String, String, String, i64, String, i64, i64, i64);

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

fn decode_row(row: CommandRow) -> Result<CommandRecord, SqliteStorageError> {
    let (
        id_str,
        name,
        action_id_str,
        cooldown_ms,
        permission,
        enabled_int,
        created_at_ms,
        last_modified_ms,
    ) = row;

    let id: CommandId = parse_id(&id_str, "command")?;
    let action_id: ActionId = parse_id(&action_id_str, "action")?;

    Ok(CommandRecord {
        id,
        name,
        action_id,
        cooldown_ms: cooldown_ms as u32,
        permission,
        enabled: enabled_int != 0,
        created_at: from_epoch_ms(created_at_ms)?,
        last_modified: from_epoch_ms(last_modified_ms)?,
    })
}

pub struct SqliteCommandRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteCommandRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CommandRepo for SqliteCommandRepo {
    async fn get(&self, id: CommandId) -> Result<Option<CommandRecord>, StorageError> {
        let id_str = id.to_string();
        let row: Option<CommandRow> = sqlx::query_as(
            "SELECT id, name, action_id, cooldown_ms, permission, enabled, created_at, last_modified
             FROM commands WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<CommandRecord>, StorageError> {
        let row: Option<CommandRow> = sqlx::query_as(
            "SELECT id, name, action_id, cooldown_ms, permission, enabled, created_at, last_modified
             FROM commands WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn upsert(&self, record: CommandRecord) -> Result<(), StorageError> {
        let id_str = record.id.to_string();
        let action_id_str = record.action_id.to_string();
        let enabled_int: i64 = if record.enabled { 1 } else { 0 };
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO commands (id, name, action_id, cooldown_ms, permission, enabled, created_at, last_modified)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name          = excluded.name,
                 action_id     = excluded.action_id,
                 cooldown_ms   = excluded.cooldown_ms,
                 permission    = excluded.permission,
                 enabled       = excluded.enabled,
                 last_modified = ?",
        )
        .bind(&id_str)
        .bind(&record.name)
        .bind(&action_id_str)
        .bind(record.cooldown_ms as i64)
        .bind(&record.permission)
        .bind(enabled_int)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: CommandId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM commands WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<CommandRecord>, StorageError> {
        let rows: Vec<CommandRow> = sqlx::query_as(
            "SELECT id, name, action_id, cooldown_ms, permission, enabled, created_at, last_modified
             FROM commands ORDER BY name",
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
    ) -> Result<Vec<CommandRecord>, StorageError> {
        let action_id_str = action_id.to_string();
        let rows: Vec<CommandRow> = sqlx::query_as(
            "SELECT id, name, action_id, cooldown_ms, permission, enabled, created_at, last_modified
             FROM commands WHERE action_id = ? ORDER BY name",
        )
        .bind(&action_id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }
}
