use async_trait::async_trait;
use forge_storage::{StorageError, TriggerRepo};
use forge_types::{ActionId, Trigger, TriggerConfig, TriggerId};

use crate::error::SqliteStorageError;

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} id '{s}': {e}")))
}

fn decode_row(
    id_str: String,
    action_id_str: String,
    kind_id: String,
    config_json: String,
) -> Result<Trigger, SqliteStorageError> {
    let id: TriggerId = parse_id(&id_str, "trigger")?;
    let action_id: ActionId = parse_id(&action_id_str, "action")?;
    let config: TriggerConfig = serde_json::from_str(&config_json)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid trigger config json: {e}")))?;

    Ok(Trigger {
        id,
        action_id,
        kind_id,
        config,
    })
}

type TriggerRow = (String, String, String, String);

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
    async fn list_for_action(&self, action_id: ActionId) -> Result<Vec<Trigger>, StorageError> {
        let action_id_str = action_id.to_string();
        let rows: Vec<TriggerRow> =
            sqlx::query_as("SELECT id, action_id, kind, config FROM triggers WHERE action_id = ?")
                .bind(&action_id_str)
                .fetch_all(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|(id, aid, kind, config)| {
                decode_row(id, aid, kind, config).map_err(StorageError::from)
            })
            .collect()
    }

    async fn save(&self, trigger: &Trigger) -> Result<(), StorageError> {
        let id_str = trigger.id.to_string();
        let action_id_str = trigger.action_id.to_string();
        let kind_str = trigger.kind_id.clone();
        let config_json =
            serde_json::to_string(&trigger.config).map_err(StorageError::Serialization)?;

        sqlx::query(
            "INSERT INTO triggers (id, action_id, kind, config)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 action_id = excluded.action_id,
                 kind      = excluded.kind,
                 config    = excluded.config",
        )
        .bind(&id_str)
        .bind(&action_id_str)
        .bind(&kind_str)
        .bind(&config_json)
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
}
