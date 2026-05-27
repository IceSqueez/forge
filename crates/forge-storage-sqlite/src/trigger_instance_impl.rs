use async_trait::async_trait;
use forge_storage::{StorageError, TriggerInstanceRepo};
use forge_types::{ActionId, TriggerInstance, TriggerInstanceId};

use crate::error::SqliteStorageError;

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} id '{s}': {e}")))
}

type InstanceRow = (String, String, String, String, i64, i64);

fn decode_row(row: InstanceRow) -> Result<TriggerInstance, SqliteStorageError> {
    let (id_str, kind_id, name, overrides_json, enabled, user_defined) = row;
    let id: TriggerInstanceId = parse_id(&id_str, "trigger_instance")?;
    let overrides = serde_json::from_str(&overrides_json)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid overrides json: {e}")))?;
    Ok(TriggerInstance {
        id,
        kind_id,
        name,
        overrides,
        enabled: enabled != 0,
        user_defined: user_defined != 0,
    })
}

pub struct SqliteTriggerInstanceRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteTriggerInstanceRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TriggerInstanceRepo for SqliteTriggerInstanceRepo {
    async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        let rows: Vec<InstanceRow> = sqlx::query_as(
            "SELECT id, kind_id, name, overrides, enabled, user_defined
             FROM trigger_instances WHERE user_defined = 1 ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerInstance>, StorageError> {
        let action_id_str = action_id.to_string();
        let rows: Vec<InstanceRow> = sqlx::query_as(
            "SELECT ti.id, ti.kind_id, ti.name, ti.overrides, ti.enabled, ti.user_defined
             FROM trigger_instances ti
             JOIN action_trigger_instances ati ON ati.trigger_instance_id = ti.id
             WHERE ati.action_id = ?
             ORDER BY ati.position",
        )
        .bind(&action_id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn actions_using(
        &self,
        instance_id: TriggerInstanceId,
    ) -> Result<Vec<ActionId>, StorageError> {
        let id_str = instance_id.to_string();
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT action_id FROM action_trigger_instances WHERE trigger_instance_id = ?",
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|(s,)| parse_id::<ActionId>(&s, "action").map_err(StorageError::from))
            .collect()
    }

    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
        let id_str = id.to_string();
        let row: Option<InstanceRow> = sqlx::query_as(
            "SELECT id, kind_id, name, overrides, enabled, user_defined
             FROM trigger_instances WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
        let id_str = instance.id.to_string();
        let overrides_json =
            serde_json::to_string(&instance.overrides).map_err(StorageError::Serialization)?;
        let enabled: i64 = if instance.enabled { 1 } else { 0 };
        let user_defined: i64 = if instance.user_defined { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 kind_id      = excluded.kind_id,
                 name         = excluded.name,
                 overrides    = excluded.overrides,
                 enabled      = excluded.enabled,
                 user_defined = excluded.user_defined",
        )
        .bind(&id_str)
        .bind(&instance.kind_id)
        .bind(&instance.name)
        .bind(&overrides_json)
        .bind(enabled)
        .bind(user_defined)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let action_ids = self.actions_using(id).await?;

        if !action_ids.is_empty() {
            let used_in_count = action_ids.len() as u32;
            let mut sample_action_names = Vec::new();
            for aid in action_ids.iter().take(3) {
                let aid_str = aid.to_string();
                let row: Option<(String,)> =
                    sqlx::query_as("SELECT name FROM actions WHERE id = ?")
                        .bind(&aid_str)
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(SqliteStorageError::Sqlx)?;
                if let Some((name,)) = row {
                    sample_action_names.push(name);
                }
            }
            return Err(StorageError::ReferenceBlock {
                used_in_count,
                sample_action_names,
            });
        }

        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM trigger_instances WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn upsert_default(
        &self,
        kind_id: &str,
        name: &str,
    ) -> Result<TriggerInstanceId, StorageError> {
        let new_id = TriggerInstanceId::new();
        let new_id_str = new_id.to_string();

        let result = sqlx::query(
            "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined)
             VALUES (?, ?, ?, '{}', 1, 0)
             ON CONFLICT(kind_id) WHERE user_defined = 0 DO NOTHING",
        )
        .bind(&new_id_str)
        .bind(kind_id)
        .bind(name)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        if result.rows_affected() == 1 {
            return Ok(new_id);
        }

        let (existing_id_str,): (String,) = sqlx::query_as(
            "SELECT id FROM trigger_instances WHERE kind_id = ? AND user_defined = 0",
        )
        .bind(kind_id)
        .fetch_one(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        parse_id::<TriggerInstanceId>(&existing_id_str, "trigger_instance")
            .map_err(StorageError::from)
    }

    async fn set_enabled(&self, id: TriggerInstanceId, enabled: bool) -> Result<(), StorageError> {
        let id_str = id.to_string();
        let enabled_val: i64 = if enabled { 1 } else { 0 };

        sqlx::query("UPDATE trigger_instances SET enabled = ? WHERE id = ?")
            .bind(enabled_val)
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }
}
