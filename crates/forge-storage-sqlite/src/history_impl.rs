use std::collections::HashMap;

use async_trait::async_trait;
use forge_storage::{ActionStats, HistoryRepo, StorageError};
use forge_types::{ActionId, ExecutionContext, ExecutionMetadata};
use serde_json;
use time::OffsetDateTime;

use crate::batch::insert_rows;
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

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

fn decode_contexts(rows: Vec<(String,)>) -> Result<Vec<ExecutionContext>, StorageError> {
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

const ACTION_HISTORY_COLUMNS: usize = 6;

struct EncodedRun {
    action_id: String,
    event_id: Option<String>,
    started_at_ms: i64,
    duration_ms: i64,
    outcome: String,
    context: String,
}

impl EncodedRun {
    fn encode(ctx: &ExecutionContext) -> Result<Self, StorageError> {
        Ok(Self {
            action_id: ctx.action_id.to_string(),
            event_id: match &ctx.metadata {
                ExecutionMetadata::Trigger { event_id, .. }
                | ExecutionMetadata::Scheduled { event_id, .. } => Some(event_id.to_string()),
                ExecutionMetadata::QuickAction { .. } => None,
            },
            started_at_ms: to_epoch_ms(ctx.started_at),
            duration_ms: ctx
                .completed_at
                .map(|finished| {
                    let diff = finished - ctx.started_at;
                    diff.whole_milliseconds().max(0) as i64
                })
                .unwrap_or(0),
            outcome: serde_json::to_string(&ctx.outcome)
                .map_err(StorageError::Serialization)?
                .trim_matches('"')
                .to_string(),
            context: serde_json::to_string(ctx).map_err(StorageError::Serialization)?,
        })
    }
}

pub struct SqliteHistoryRepo {
    db: SqlitePools,
}

impl SqliteHistoryRepo {
    pub(crate) async fn prune_chunk_before(
        &self,
        cutoff: OffsetDateTime,
        max_rows: u32,
    ) -> Result<u64, SqliteStorageError> {
        let result = sqlx::query(
            "DELETE FROM action_history WHERE rowid IN (
                 SELECT rowid FROM action_history WHERE started_at < ? LIMIT ?
             )",
        )
        .bind(to_epoch_ms(cutoff))
        .bind(i64::from(max_rows))
        .execute(self.db.writer())
        .await?;
        Ok(result.rows_affected())
    }

    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[async_trait]
impl HistoryRepo for SqliteHistoryRepo {
    async fn save(&self, ctx: &ExecutionContext) -> Result<(), StorageError> {
        let row = EncodedRun::encode(ctx)?;
        sqlx::query(
            "INSERT INTO action_history
                (action_id, triggering_event_id, started_at, duration_ms, outcome, context)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.action_id)
        .bind(row.event_id.as_deref())
        .bind(row.started_at_ms)
        .bind(row.duration_ms)
        .bind(&row.outcome)
        .bind(&row.context)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn save_batch(&self, contexts: &[ExecutionContext]) -> Result<(), StorageError> {
        if contexts.is_empty() {
            return Ok(());
        }
        let rows = contexts
            .iter()
            .map(EncodedRun::encode)
            .collect::<Result<Vec<_>, _>>()?;

        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        insert_rows(
            &mut tx,
            "INSERT INTO action_history
                (action_id, triggering_event_id, started_at, duration_ms, outcome, context) ",
            ACTION_HISTORY_COLUMNS,
            "",
            &rows,
            |values, row| {
                values
                    .push_bind(row.action_id.as_str())
                    .push_bind(row.event_id.as_deref())
                    .push_bind(row.started_at_ms)
                    .push_bind(row.duration_ms)
                    .push_bind(row.outcome.as_str())
                    .push_bind(row.context.as_str());
            },
        )
        .await?;
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
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
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_contexts(rows)
    }

    async fn recent_for_builtin(
        &self,
        builtin_id: &str,
        limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT context FROM action_history
             WHERE json_extract(context, '$.metadata.kind') = 'quick_action'
               AND json_extract(context, '$.metadata.builtin_id') = ?
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(builtin_id)
        .bind(limit as i64)
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_contexts(rows)
    }

    async fn recent(&self, limit: u32) -> Result<Vec<ExecutionContext>, StorageError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT context FROM action_history
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        decode_contexts(rows)
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
             WHERE triggering_event_id IS NOT NULL
             GROUP BY action_id",
        )
        .bind(since_ms)
        .fetch_all(self.db.reader())
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{EventId, ExecutionOutcome};

    use super::*;
    use crate::{apply_migrations, connect};

    const CUTOFF_UNIX_SECS: i64 = 1_700_000_000;

    async fn make_repo() -> SqliteHistoryRepo {
        let pool = connect(":memory:").await.unwrap();
        apply_migrations(&pool).await.unwrap();
        SqliteHistoryRepo::new(pool)
    }

    fn cutoff() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(CUTOFF_UNIX_SECS).unwrap()
    }

    fn run_started_at(started_at: OffsetDateTime) -> ExecutionContext {
        ExecutionContext {
            action_id: ActionId::new(),
            metadata: ExecutionMetadata::Trigger {
                event_id: EventId::new(),
                trigger_kind: None,
            },
            arg_stack_snapshot: BTreeMap::new(),
            started_at,
            completed_at: Some(started_at),
            telemetry: vec![],
            outcome: ExecutionOutcome::Success,
        }
    }

    async fn surviving_start_ms(repo: &SqliteHistoryRepo) -> Vec<i64> {
        sqlx::query_scalar("SELECT started_at FROM action_history ORDER BY started_at")
            .fetch_all(repo.db.writer())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn prune_chunk_deletes_only_runs_strictly_older_than_the_cutoff_millisecond() {
        let repo = make_repo().await;
        let offsets_ms = [-1_000_i64, -1, 0, 1];
        let runs: Vec<_> = offsets_ms
            .iter()
            .map(|ms| run_started_at(cutoff() + time::Duration::milliseconds(*ms)))
            .collect();
        repo.save_batch(&runs).await.unwrap();

        let pruned = repo.prune_chunk_before(cutoff(), 100).await.unwrap();

        assert_eq!(pruned, 2);
        let cutoff_ms = CUTOFF_UNIX_SECS * 1_000;
        assert_eq!(
            surviving_start_ms(&repo).await,
            vec![cutoff_ms, cutoff_ms + 1]
        );
    }

    #[tokio::test]
    async fn prune_chunk_deletes_at_most_max_rows_per_call_and_never_newer_runs() {
        let repo = make_repo().await;
        let mut runs: Vec<_> = (1..=5)
            .map(|days| run_started_at(cutoff() - time::Duration::days(days)))
            .collect();
        runs.push(run_started_at(cutoff() + time::Duration::days(1)));
        repo.save_batch(&runs).await.unwrap();

        let mut per_call = Vec::new();
        for _ in 0..4 {
            per_call.push(repo.prune_chunk_before(cutoff(), 2).await.unwrap());
        }

        assert_eq!(per_call, vec![2, 2, 1, 0]);
        assert_eq!(
            surviving_start_ms(&repo).await,
            vec![to_epoch_ms(cutoff() + time::Duration::days(1))]
        );
    }

    #[tokio::test]
    async fn prune_chunk_with_zero_max_rows_deletes_nothing() {
        let repo = make_repo().await;
        repo.save(&run_started_at(cutoff() - time::Duration::days(1)))
            .await
            .unwrap();

        let pruned = repo.prune_chunk_before(cutoff(), 0).await.unwrap();

        assert_eq!(pruned, 0);
        assert_eq!(surviving_start_ms(&repo).await.len(), 1);
    }
}
