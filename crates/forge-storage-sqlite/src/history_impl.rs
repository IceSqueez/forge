use async_trait::async_trait;
use forge_storage::{HistoryRepo, StorageError};
use forge_types::{ActionId, ExecutionContext};
use serde_json;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / 1_000_000) as i64
}

pub struct SqliteHistoryRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteHistoryRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HistoryRepo for SqliteHistoryRepo {
    async fn save(&self, ctx: &ExecutionContext) -> Result<(), StorageError> {
        let action_id_str = ctx.action_id.to_string();
        let event_id_str = ctx.trigger_event_id.to_string();
        let started_at_ms = to_epoch_ms(ctx.started_at);
        let duration_ms = ctx
            .completed_at
            .map(|finished| {
                let diff = finished - ctx.started_at;
                diff.whole_milliseconds().max(0) as i64
            })
            .unwrap_or(0);
        let outcome_str = serde_json::to_string(&ctx.outcome)
            .map_err(StorageError::Serialization)?
            .trim_matches('"')
            .to_string();
        let context_json = serde_json::to_string(ctx).map_err(StorageError::Serialization)?;

        sqlx::query(
            "INSERT INTO action_history
                (action_id, triggering_event_id, started_at, duration_ms, outcome, context)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&action_id_str)
        .bind(&event_id_str)
        .bind(started_at_ms)
        .bind(duration_ms)
        .bind(&outcome_str)
        .bind(&context_json)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn recent_for_action(
        &self,
        action_id: ActionId,
        limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        let action_id_str = action_id.to_string();
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT context FROM action_history
             WHERE action_id = ?
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(&action_id_str)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|(ctx_json,)| {
                serde_json::from_str::<ExecutionContext>(&ctx_json).map_err(|e| {
                    StorageError::from(SqliteStorageError::Decode(format!(
                        "invalid ExecutionContext json: {e}"
                    )))
                })
            })
            .collect()
    }
}
