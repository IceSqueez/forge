use async_trait::async_trait;
use forge_storage::{ActionRepo, ActionTelemetry, ExecutionStatus, StorageError};
use forge_types::{Action, ActionId, ExecutionMode, QueueId, SubActionStep};
use serde_json;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} id '{s}': {e}")))
}

#[derive(sqlx::FromRow)]
struct ActionRow {
    id: String,
    name: String,
    group_name: String,
    queue_id: String,
    enabled: i64,
    concurrent: i64,
    bypass_pause: i64,
    description: String,
    sub_actions: String,
    execution_mode: String,
}

fn parse_execution_mode(s: &str) -> ExecutionMode {
    match s {
        "random_pick" => ExecutionMode::RandomPick,
        _ => ExecutionMode::Sequential,
    }
}

fn encode_execution_mode(m: ExecutionMode) -> &'static str {
    match m {
        ExecutionMode::Sequential => "sequential",
        ExecutionMode::RandomPick => "random_pick",
    }
}

fn decode_row(row: ActionRow) -> Result<Action, SqliteStorageError> {
    let id: ActionId = parse_id(&row.id, "action")?;
    let queue_id: QueueId = parse_id(&row.queue_id, "queue")?;
    let sub_actions: Vec<SubActionStep> = serde_json::from_str(&row.sub_actions)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid sub_actions json: {e}")))?;

    Ok(Action {
        id,
        name: row.name,
        group: if row.group_name.is_empty() {
            None
        } else {
            Some(row.group_name)
        },
        queue_id,
        enabled: row.enabled != 0,
        concurrent: row.concurrent != 0,
        bypass_pause: row.bypass_pause != 0,
        execution_mode: parse_execution_mode(&row.execution_mode),
        description: if row.description.is_empty() {
            None
        } else {
            Some(row.description)
        },
        sub_actions,
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
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        let rows: Vec<ActionRow> = sqlx::query_as(
            "SELECT id, name, group_name, queue_id, enabled, concurrent, bypass_pause, description, sub_actions, execution_mode
             FROM actions WHERE archived_at IS NULL ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        let id_str = id.to_string();
        let row: Option<ActionRow> = sqlx::query_as(
            "SELECT id, name, group_name, queue_id, enabled, concurrent, bypass_pause, description, sub_actions, execution_mode
             FROM actions WHERE id = ? AND archived_at IS NULL",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|row| decode_row(row).map_err(StorageError::from))
            .transpose()
    }

    async fn save(&self, action: &Action) -> Result<(), StorageError> {
        let id_str = action.id.to_string();
        let queue_id_str = action.queue_id.to_string();
        let group_name = action.group.as_deref().unwrap_or("").to_string();
        let description = action.description.as_deref().unwrap_or("").to_string();
        let sub_actions_json =
            serde_json::to_string(&action.sub_actions).map_err(StorageError::Serialization)?;
        let enabled: i64 = if action.enabled { 1 } else { 0 };
        let concurrent: i64 = if action.concurrent { 1 } else { 0 };
        let bypass_pause: i64 = if action.bypass_pause { 1 } else { 0 };
        let execution_mode = encode_execution_mode(action.execution_mode);

        sqlx::query(
            "INSERT INTO actions (id, name, group_name, queue_id, enabled, concurrent, bypass_pause, description, sub_actions, execution_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 name           = excluded.name,
                 group_name     = excluded.group_name,
                 queue_id       = excluded.queue_id,
                 enabled        = excluded.enabled,
                 concurrent     = excluded.concurrent,
                 bypass_pause   = excluded.bypass_pause,
                 description    = excluded.description,
                 sub_actions    = excluded.sub_actions,
                 execution_mode = excluded.execution_mode",
        )
        .bind(&id_str)
        .bind(&action.name)
        .bind(&group_name)
        .bind(&queue_id_str)
        .bind(enabled)
        .bind(concurrent)
        .bind(bypass_pause)
        .bind(&description)
        .bind(&sub_actions_json)
        .bind(execution_mode)
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

    async fn list_by_group<'a>(
        &'a self,
        group: Option<&'a str>,
    ) -> Result<Vec<Action>, StorageError> {
        let group_val = group.unwrap_or("");
        let rows: Vec<ActionRow> = sqlx::query_as(
            "SELECT id, name, group_name, queue_id, enabled, concurrent, bypass_pause, description, sub_actions, execution_mode
             FROM actions WHERE group_name = ? AND archived_at IS NULL ORDER BY name",
        )
        .bind(group_val)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn record_execution(
        &self,
        action_id: ActionId,
        started_at: OffsetDateTime,
        duration_ms: u64,
        status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        let id_str = action_id.to_string();
        let started_at_secs = started_at.unix_timestamp();
        let duration_i64 = duration_ms as i64;
        let status_str = match status {
            ExecutionStatus::Success => "ok",
            ExecutionStatus::Error => "err",
        };
        sqlx::query(
            "INSERT INTO action_executions (action_id, started_at, duration_ms, status)
             VALUES (?, ?, ?, ?)",
        )
        .bind(id_str)
        .bind(started_at_secs)
        .bind(duration_i64)
        .bind(status_str)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn prune_executions_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        let cutoff_secs = cutoff.unix_timestamp();
        let result = sqlx::query("DELETE FROM action_executions WHERE started_at < ?")
            .bind(cutoff_secs)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected())
    }

    async fn duplicate(
        &self,
        source_id: ActionId,
        new_id: ActionId,
        new_name: &str,
    ) -> Result<(), StorageError> {
        let source_id_str = source_id.to_string();
        let new_id_str = new_id.to_string();

        let mut tx = self.pool.begin().await.map_err(SqliteStorageError::Sqlx)?;

        let row: Option<ActionRow> = sqlx::query_as(
            "SELECT id, name, group_name, queue_id, enabled, concurrent, bypass_pause, description, sub_actions, execution_mode
             FROM actions WHERE id = ?",
        )
        .bind(&source_id_str)
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let row = row.ok_or_else(|| StorageError::NotFound {
            key: source_id_str.clone(),
        })?;

        sqlx::query(
            "INSERT INTO actions (id, name, group_name, queue_id, enabled, concurrent, bypass_pause, description, sub_actions, execution_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&new_id_str)
        .bind(new_name)
        .bind(&row.group_name)
        .bind(&row.queue_id)
        .bind(row.enabled)
        .bind(row.concurrent)
        .bind(row.bypass_pause)
        .bind(&row.description)
        .bind(&row.sub_actions)
        .bind(&row.execution_mode)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        sqlx::query(
            "INSERT INTO action_trigger_instances (action_id, trigger_instance_id, position)
             SELECT ?, trigger_instance_id, position FROM action_trigger_instances WHERE action_id = ?",
        )
        .bind(&new_id_str)
        .bind(&source_id_str)
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn telemetry(&self, id: ActionId) -> Result<ActionTelemetry, StorageError> {
        let id_str = id.to_string();
        let now = OffsetDateTime::now_utc();
        let start_of_today = now.replace_time(time::Time::MIDNIGHT).unix_timestamp();
        let start_of_7d = (now - time::Duration::days(7)).unix_timestamp();

        #[derive(sqlx::FromRow)]
        struct TelemetryRow {
            last_fired_at: Option<i64>,
            runs_today: i64,
            avg_duration_ms: Option<f64>,
            errors_7d: i64,
        }

        let row: TelemetryRow = sqlx::query_as(
            "WITH \
               lf AS (SELECT MAX(started_at) AS v \
                      FROM action_executions WHERE action_id = ?), \
               rt AS (SELECT COUNT(*) AS v \
                      FROM action_executions \
                      WHERE action_id = ? AND started_at >= ?), \
               ad AS (SELECT AVG(duration_ms) AS v \
                      FROM (SELECT duration_ms FROM action_executions \
                            WHERE action_id = ? ORDER BY started_at DESC LIMIT 100)), \
               e7 AS (SELECT COUNT(*) AS v \
                      FROM action_executions \
                      WHERE action_id = ? AND status = 'err' AND started_at >= ?) \
             SELECT lf.v AS last_fired_at, rt.v AS runs_today, \
                    ad.v AS avg_duration_ms, e7.v AS errors_7d FROM lf, rt, ad, e7",
        )
        .bind(&id_str)
        .bind(&id_str)
        .bind(start_of_today)
        .bind(&id_str)
        .bind(&id_str)
        .bind(start_of_7d)
        .fetch_one(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(ActionTelemetry {
            last_fired_at: row
                .last_fired_at
                .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok()),
            runs_today: row.runs_today.max(0) as u64,
            avg_duration_ms: row.avg_duration_ms.map(|v| v.round() as u64),
            errors_7d: row.errors_7d.max(0) as u64,
        })
    }

    async fn archive(&self, id: ActionId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let result =
            sqlx::query("UPDATE actions SET archived_at = ? WHERE id = ? AND archived_at IS NULL")
                .bind(now_ms)
                .bind(&id_str)
                .execute(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn restore(&self, id: ActionId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query(
            "UPDATE actions SET archived_at = NULL WHERE id = ? AND archived_at IS NOT NULL",
        )
        .bind(&id_str)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_archived(&self) -> Result<Vec<Action>, StorageError> {
        let rows: Vec<ActionRow> = sqlx::query_as(
            "SELECT id, name, group_name, queue_id, enabled, concurrent, bypass_pause, description, sub_actions, execution_mode
             FROM actions WHERE archived_at IS NOT NULL ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }
}
