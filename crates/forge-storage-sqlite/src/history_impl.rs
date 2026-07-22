use std::collections::HashMap;

use async_trait::async_trait;
use forge_storage::{ActionStats, HistoryRepo, StorageError};
use forge_types::{ActionId, ExecutionContext, ExecutionMetadata};
use serde_json;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn parse_action_id(s: &str) -> Result<ActionId, StorageError> {
    serde_json::from_str(&format!("\"{s}\"")).map_err(|e| {
        StorageError::from(SqliteStorageError::Decode(format!(
            "invalid action id `{s}`: {e}"
        )))
    })
}

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
        let event_id_str: Option<String> = match &ctx.metadata {
            ExecutionMetadata::Trigger { event_id, .. } => Some(event_id.to_string()),
            ExecutionMetadata::QuickAction { .. } => None,
        };
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
        .bind(event_id_str.as_deref())
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

    async fn stats_summary(
        &self,
        since: OffsetDateTime,
    ) -> Result<HashMap<ActionId, ActionStats>, StorageError> {
        let since_ms = to_epoch_ms(since);

        #[derive(sqlx::FromRow)]
        struct StatsSummaryRow {
            action_id: String,
            last_started: i64,
            runs_24h: i64,
        }

        let rows: Vec<StatsSummaryRow> = sqlx::query_as(
            "SELECT action_id,
                    MAX(started_at) AS last_started,
                    SUM(CASE WHEN started_at >= ? THEN 1 ELSE 0 END) AS runs_24h
             FROM action_history
             GROUP BY action_id",
        )
        .bind(since_ms)
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let mut out = HashMap::with_capacity(rows.len());
        for row in rows {
            let id = parse_action_id(&row.action_id)?;
            let last_ran_at =
                OffsetDateTime::from_unix_timestamp_nanos(i128::from(row.last_started) * 1_000_000)
                    .map_err(|e| {
                        StorageError::from(SqliteStorageError::Decode(format!(
                            "invalid started_at {}: {e}",
                            row.last_started
                        )))
                    })?;
            out.insert(
                id,
                ActionStats {
                    last_ran_at,
                    runs_24h: u32::try_from(row.runs_24h).unwrap_or(u32::MAX),
                },
            );
        }
        Ok(out)
    }

    async fn prune_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        let cutoff_ms = to_epoch_ms(cutoff);
        let result = sqlx::query("DELETE FROM action_history WHERE started_at < ?")
            .bind(cutoff_ms)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected())
    }
}
