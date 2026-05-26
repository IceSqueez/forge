use async_trait::async_trait;
use forge_storage::{ActionRepo, ActionTelemetry, StorageError};
use forge_types::{Action, ActionId, ExecutionMode, QueueId, SubActionSpec};
use serde_json;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn parse_id<T: serde::de::DeserializeOwned>(s: &str, label: &str) -> Result<T, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid {label} id '{s}': {e}")))
}

type ActionRow = (
    String,
    String,
    String,
    String,
    i64,
    i64,
    i64,
    String,
    String,
    String,
);

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
    let (
        id_str,
        name,
        group_name,
        queue_id_str,
        enabled,
        concurrent,
        bypass_pause,
        description,
        sub_actions_json,
        execution_mode_str,
    ) = row;
    let id: ActionId = parse_id(&id_str, "action")?;
    let queue_id: QueueId = parse_id(&queue_id_str, "queue")?;
    let sub_actions: Vec<SubActionSpec> = serde_json::from_str(&sub_actions_json)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid sub_actions json: {e}")))?;

    Ok(Action {
        id,
        name,
        group: if group_name.is_empty() {
            None
        } else {
            Some(group_name)
        },
        queue_id,
        enabled: enabled != 0,
        concurrent: concurrent != 0,
        bypass_pause: bypass_pause != 0,
        execution_mode: parse_execution_mode(&execution_mode_str),
        description: if description.is_empty() {
            None
        } else {
            Some(description)
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
             FROM actions ORDER BY name",
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
             FROM actions WHERE id = ?",
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
             FROM actions WHERE group_name = ? ORDER BY name",
        )
        .bind(group_val)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn telemetry(&self, id: ActionId) -> Result<ActionTelemetry, StorageError> {
        let id_str = id.to_string();
        let now = OffsetDateTime::now_utc();
        let start_of_today = now.replace_time(time::Time::MIDNIGHT).unix_timestamp();
        let start_of_7d = (now - time::Duration::days(7)).unix_timestamp();

        type TelemetryRow = (Option<i64>, i64, Option<f64>, i64);

        let (last_fired_raw, runs_today_raw, avg_dur_raw, errors_7d_raw): TelemetryRow =
            sqlx::query_as(
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
                 SELECT lf.v, rt.v, ad.v, e7.v FROM lf, rt, ad, e7",
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
            last_fired_at: last_fired_raw
                .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok()),
            runs_today: runs_today_raw.max(0) as u64,
            avg_duration_ms: avg_dur_raw.map(|v| v.round() as u64),
            errors_7d: errors_7d_raw.max(0) as u64,
        })
    }
}
