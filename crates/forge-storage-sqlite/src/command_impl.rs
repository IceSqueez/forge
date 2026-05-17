use async_trait::async_trait;
use forge_storage::{CommandRepo, StorageError};
use forge_types::{Command, CommandId, CommandPermission};
use serde_json;

use crate::error::SqliteStorageError;

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} id '{s}': {e}")))
}

fn decode_row(
    id_str: String,
    action_id_str: String,
    name: String,
    cooldown_secs: i64,
    permission_str: String,
) -> Result<Command, SqliteStorageError> {
    let id: CommandId = parse_id(&id_str, "command")?;
    let action_id = parse_id(&action_id_str, "action")?;
    let permission: CommandPermission = serde_json::from_str(&format!("\"{permission_str}\""))
        .map_err(|e| {
            SqliteStorageError::Decode(format!("invalid permission '{permission_str}': {e}"))
        })?;

    Ok(Command {
        id,
        action_id,
        name,
        cooldown_secs: cooldown_secs as u64,
        permission,
    })
}

type CommandRow = (String, String, String, i64, String);

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
    async fn list(&self) -> Result<Vec<Command>, StorageError> {
        let rows: Vec<CommandRow> = sqlx::query_as(
            "SELECT id, action_id, name, cooldown_secs, permission FROM commands ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|(id, aid, name, cooldown, perm)| {
                decode_row(id, aid, name, cooldown, perm).map_err(StorageError::from)
            })
            .collect()
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<Command>, StorageError> {
        let row: Option<CommandRow> = sqlx::query_as(
            "SELECT id, action_id, name, cooldown_secs, permission FROM commands WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|(id, aid, name, cooldown, perm)| {
            decode_row(id, aid, name, cooldown, perm).map_err(StorageError::from)
        })
        .transpose()
    }

    async fn save(&self, command: &Command) -> Result<(), StorageError> {
        let id_str = command.id.to_string();
        let action_id_str = command.action_id.to_string();
        let permission_str = serde_json::to_string(&command.permission)
            .map_err(StorageError::Serialization)?
            .trim_matches('"')
            .to_string();

        sqlx::query(
            "INSERT INTO commands (id, action_id, name, cooldown_secs, permission)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 action_id     = excluded.action_id,
                 name          = excluded.name,
                 cooldown_secs = excluded.cooldown_secs,
                 permission    = excluded.permission",
        )
        .bind(&id_str)
        .bind(&action_id_str)
        .bind(&command.name)
        .bind(command.cooldown_secs as i64)
        .bind(&permission_str)
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
}
