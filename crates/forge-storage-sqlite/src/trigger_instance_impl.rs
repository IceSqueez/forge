use std::str::FromStr;

use async_trait::async_trait;
use forge_storage::{StorageError, TriggerInstanceRepo};
use forge_types::{ActionId, PermissionRung, PlatformScope, TriggerInstance, TriggerInstanceId};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} id '{s}': {e}")))
}

#[derive(sqlx::FromRow)]
struct InstanceRow {
    id: String,
    kind_id: String,
    name: String,
    overrides: String,
    enabled: i64,
    user_defined: i64,
    platform_scope: String,
    cooldown_secs: i64,
    cooldown_global: i64,
    permission_rung: String,
}

fn decode_row(row: InstanceRow) -> Result<TriggerInstance, SqliteStorageError> {
    let id: TriggerInstanceId = parse_id(&row.id, "trigger_instance")?;
    let overrides = serde_json::from_str(&row.overrides)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid overrides json: {e}")))?;
    let platform_scope = serde_json::from_str::<PlatformScope>(&row.platform_scope)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid platform_scope json: {e}")))?;
    let permission_rung = PermissionRung::from_str(&row.permission_rung).map_err(|e| {
        SqliteStorageError::Decode(format!(
            "invalid permission_rung '{}': {e}",
            row.permission_rung
        ))
    })?;
    Ok(TriggerInstance {
        id,
        kind_id: row.kind_id,
        name: row.name,
        overrides,
        enabled: row.enabled != 0,
        user_defined: row.user_defined != 0,
        platform_scope,
        cooldown_secs: row.cooldown_secs.max(0) as u32,
        cooldown_global: row.cooldown_global != 0,
        permission_rung,
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
    async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        let rows: Vec<InstanceRow> = sqlx::query_as(
            "SELECT id, kind_id, name, overrides, enabled, user_defined, platform_scope,
                    cooldown_secs, cooldown_global, permission_rung
             FROM trigger_instances
             WHERE archived_at IS NULL
             ORDER BY user_defined ASC, name ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        let rows: Vec<InstanceRow> = sqlx::query_as(
            "SELECT id, kind_id, name, overrides, enabled, user_defined, platform_scope,
                    cooldown_secs, cooldown_global, permission_rung
             FROM trigger_instances WHERE user_defined = 1 AND archived_at IS NULL ORDER BY name",
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
            "SELECT ti.id, ti.kind_id, ti.name, ti.overrides, ti.enabled, ti.user_defined,
                    ti.platform_scope, ti.cooldown_secs, ti.cooldown_global, ti.permission_rung
             FROM trigger_instances ti
             JOIN action_trigger_instances ati ON ati.trigger_instance_id = ti.id
             WHERE ati.action_id = ? AND ti.archived_at IS NULL
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
            "SELECT ati.action_id
             FROM action_trigger_instances ati
             JOIN actions a ON a.id = ati.action_id
             WHERE ati.trigger_instance_id = ? AND a.archived_at IS NULL",
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|(s,)| parse_id::<ActionId>(&s, "action").map_err(StorageError::from))
            .collect()
    }

    async fn link_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
        position: i64,
    ) -> Result<(), StorageError> {
        let action_id_str = action_id.to_string();
        let instance_id_str = instance_id.to_string();

        let result = sqlx::query(
            "INSERT INTO action_trigger_instances (action_id, trigger_instance_id, position)
             VALUES (?, ?, ?)",
        )
        .bind(&action_id_str)
        .bind(&instance_id_str)
        .bind(position)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => Ok(()),
            Err(e) => Err(StorageError::from(SqliteStorageError::Sqlx(e))),
        }
    }

    async fn unlink_action(
        &self,
        action_id: ActionId,
        instance_id: TriggerInstanceId,
    ) -> Result<bool, StorageError> {
        let action_id_str = action_id.to_string();
        let instance_id_str = instance_id.to_string();

        let result = sqlx::query(
            "DELETE FROM action_trigger_instances
             WHERE action_id = ? AND trigger_instance_id = ?",
        )
        .bind(&action_id_str)
        .bind(&instance_id_str)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
        let id_str = id.to_string();
        let row: Option<InstanceRow> = sqlx::query_as(
            "SELECT id, kind_id, name, overrides, enabled, user_defined, platform_scope,
                    cooldown_secs, cooldown_global, permission_rung
             FROM trigger_instances WHERE id = ? AND archived_at IS NULL",
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
        let platform_scope_json =
            serde_json::to_string(&instance.platform_scope).map_err(StorageError::Serialization)?;
        let enabled: i64 = if instance.enabled { 1 } else { 0 };
        let user_defined: i64 = if instance.user_defined { 1 } else { 0 };

        let cooldown_secs: i64 = instance.cooldown_secs.into();
        let cooldown_global: i64 = if instance.cooldown_global { 1 } else { 0 };
        let permission_rung = instance.permission_rung.as_str();

        sqlx::query(
            "INSERT INTO trigger_instances
                 (id, kind_id, name, overrides, enabled, user_defined, platform_scope,
                  cooldown_secs, cooldown_global, permission_rung)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 kind_id         = excluded.kind_id,
                 name            = excluded.name,
                 overrides       = excluded.overrides,
                 enabled         = excluded.enabled,
                 user_defined    = excluded.user_defined,
                 platform_scope  = excluded.platform_scope,
                 cooldown_secs   = excluded.cooldown_secs,
                 cooldown_global = excluded.cooldown_global,
                 permission_rung = excluded.permission_rung",
        )
        .bind(&id_str)
        .bind(&instance.kind_id)
        .bind(&instance.name)
        .bind(&overrides_json)
        .bind(enabled)
        .bind(user_defined)
        .bind(&platform_scope_json)
        .bind(cooldown_secs)
        .bind(cooldown_global)
        .bind(permission_rung)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        // Deliberately unfiltered by archived_at: the FK is ON DELETE RESTRICT regardless
        // of the linked action's archive state, so a link from an archived action must
        // still block this hard delete.
        let id_str_probe = id.to_string();
        let linked_rows: Vec<(String,)> = sqlx::query_as(
            "SELECT action_id FROM action_trigger_instances WHERE trigger_instance_id = ?",
        )
        .bind(&id_str_probe)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        let action_ids = linked_rows
            .into_iter()
            .map(|(s,)| parse_id::<ActionId>(&s, "action").map_err(StorageError::from))
            .collect::<Result<Vec<_>, _>>()?;

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
            "INSERT INTO trigger_instances
                 (id, kind_id, name, overrides, enabled, user_defined, platform_scope)
             VALUES (?, ?, ?, '{}', 1, 0, '\"any\"')
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

    async fn archive(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let result = sqlx::query(
            "UPDATE trigger_instances SET archived_at = ? WHERE id = ? AND archived_at IS NULL",
        )
        .bind(now_ms)
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn restore(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query(
            "UPDATE trigger_instances SET archived_at = NULL WHERE id = ? AND archived_at IS NOT NULL",
        )
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_archived(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        let rows: Vec<InstanceRow> = sqlx::query_as(
            "SELECT id, kind_id, name, overrides, enabled, user_defined, platform_scope,
                    cooldown_secs, cooldown_global, permission_rung
             FROM trigger_instances
             WHERE archived_at IS NOT NULL
             ORDER BY user_defined ASC, name ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn row(permission_rung: &str) -> InstanceRow {
        InstanceRow {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_owned(),
            kind_id: "twitch.chat.command".to_owned(),
            name: "Gated".to_owned(),
            overrides: "{}".to_owned(),
            enabled: 1,
            user_defined: 1,
            platform_scope: "\"any\"".to_owned(),
            cooldown_secs: 0,
            cooldown_global: 1,
            permission_rung: permission_rung.to_owned(),
        }
    }

    #[test]
    fn a_foreign_permission_rung_string_fails_the_row_decode() {
        // A rung this build does not know must surface as a typed decode error rather than
        // silently collapsing to the floor and quietly opening a gated command.
        for foreign in ["regular", "owner", "", "Moderator"] {
            let err = decode_row(row(foreign)).expect_err("must not decode");
            assert!(
                matches!(&err, SqliteStorageError::Decode(msg) if msg.contains(foreign)),
                "expected a decode error naming {foreign:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn a_canonical_permission_rung_string_decodes_to_its_rung() {
        for rung in [
            PermissionRung::Everyone,
            PermissionRung::Subscriber,
            PermissionRung::Vip,
            PermissionRung::Moderator,
            PermissionRung::Broadcaster,
        ] {
            let decoded = decode_row(row(rung.as_str())).expect("must decode");
            assert_eq!(decoded.permission_rung, rung);
        }
    }
}
