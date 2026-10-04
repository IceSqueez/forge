use std::time::Duration;

use async_trait::async_trait;
use forge_storage::{
    MissedRunPolicy, ScheduledRun, ScheduledRunId, ScheduledRunOutcome, ScheduledRunPlacement,
    ScheduledRunRepo, ScheduledRunSpec, ScheduledRunState, StorageError,
};
use forge_types::{ActionId, EventId, Variant};
use sqlx::SqliteConnection;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

const NANOS_PER_MILLI: i128 = 1_000_000;

const POLICY_RUN_LATE_ONCE: &str = "run_late_once";
const POLICY_SKIP_IF_LATE_BY: &str = "skip_if_late_by";

const STATE_PENDING: &str = "pending";
const STATE_DISPATCHED: &str = "dispatched";
const STATE_CANCELLED: &str = "cancelled";
const STATE_SKIPPED: &str = "skipped";
const STATE_FAILED: &str = "failed";

const REASON_SUPERSEDED: &str = "superseded";
const REASON_CANCELLED: &str = "cancelled";

macro_rules! columns {
    () => {
        "id, target_action_id, due_at, key, missed_policy, skip_late_ms, args, scheduled_by_action, scheduled_by_run, trigger_event_id, scheduled_at, label, state, outcome_reason, resolved_at"
    };
}

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / NANOS_PER_MILLI) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * NANOS_PER_MILLI)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch {ms}: {e}")))
}

fn state_label(state: ScheduledRunState) -> &'static str {
    match state {
        ScheduledRunState::Pending => STATE_PENDING,
        ScheduledRunState::Dispatched => STATE_DISPATCHED,
        ScheduledRunState::Cancelled => STATE_CANCELLED,
        ScheduledRunState::Skipped => STATE_SKIPPED,
        ScheduledRunState::Failed => STATE_FAILED,
    }
}

fn parse_state(label: &str) -> Result<ScheduledRunState, SqliteStorageError> {
    match label {
        STATE_PENDING => Ok(ScheduledRunState::Pending),
        STATE_DISPATCHED => Ok(ScheduledRunState::Dispatched),
        STATE_CANCELLED => Ok(ScheduledRunState::Cancelled),
        STATE_SKIPPED => Ok(ScheduledRunState::Skipped),
        STATE_FAILED => Ok(ScheduledRunState::Failed),
        other => Err(SqliteStorageError::Decode(format!(
            "unknown scheduled run state '{other}'"
        ))),
    }
}

fn policy_columns(policy: MissedRunPolicy) -> Result<(&'static str, Option<i64>), StorageError> {
    match policy {
        MissedRunPolicy::RunLateOnce => Ok((POLICY_RUN_LATE_ONCE, None)),
        MissedRunPolicy::SkipIfLateBy(limit) => {
            let millis =
                i64::try_from(limit.as_millis()).map_err(|_| StorageError::ValidationFailed {
                    field: "missed_run_policy".to_owned(),
                    reason: "skip threshold is too large".to_owned(),
                })?;
            Ok((POLICY_SKIP_IF_LATE_BY, Some(millis)))
        }
    }
}

fn parse_policy(
    label: &str,
    skip_late_ms: Option<i64>,
) -> Result<MissedRunPolicy, SqliteStorageError> {
    match (label, skip_late_ms) {
        (POLICY_RUN_LATE_ONCE, None) => Ok(MissedRunPolicy::RunLateOnce),
        (POLICY_SKIP_IF_LATE_BY, Some(ms)) => {
            let millis = u64::try_from(ms)
                .map_err(|_| SqliteStorageError::Decode(format!("negative skip threshold {ms}")))?;
            Ok(MissedRunPolicy::SkipIfLateBy(Duration::from_millis(millis)))
        }
        (other, _) => Err(SqliteStorageError::Decode(format!(
            "unknown missed run policy '{other}'"
        ))),
    }
}

fn parse_action_id(raw: &str) -> Result<ActionId, SqliteStorageError> {
    raw.parse()
        .map_err(|e| SqliteStorageError::Decode(format!("invalid action id '{raw}': {e}")))
}

fn parse_event_id(raw: &str) -> Result<EventId, SqliteStorageError> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid event id '{raw}': {e}")))
}

#[derive(sqlx::FromRow)]
struct ScheduledRunRow {
    id: i64,
    target_action_id: String,
    due_at: i64,
    key: Option<String>,
    missed_policy: String,
    skip_late_ms: Option<i64>,
    args: String,
    scheduled_by_action: Option<String>,
    scheduled_by_run: Option<String>,
    trigger_event_id: Option<String>,
    scheduled_at: i64,
    label: String,
    state: String,
    outcome_reason: Option<String>,
    resolved_at: Option<i64>,
}

fn decode_row(row: ScheduledRunRow) -> Result<ScheduledRun, StorageError> {
    Ok(ScheduledRun {
        id: ScheduledRunId::new(row.id),
        spec: ScheduledRunSpec {
            target_action_id: parse_action_id(&row.target_action_id)?,
            due_at: from_epoch_ms(row.due_at)?,
            key: row.key,
            missed_run_policy: parse_policy(&row.missed_policy, row.skip_late_ms)?,
            args: serde_json::from_str(&row.args)?,
            scheduled_by_action: row
                .scheduled_by_action
                .as_deref()
                .map(parse_action_id)
                .transpose()?,
            scheduled_by_run: row.scheduled_by_run,
            trigger_event_id: row
                .trigger_event_id
                .as_deref()
                .map(parse_event_id)
                .transpose()?,
            scheduled_at: from_epoch_ms(row.scheduled_at)?,
            label: row.label,
        },
        state: parse_state(&row.state)?,
        outcome_reason: row.outcome_reason,
        resolved_at: row.resolved_at.map(from_epoch_ms).transpose()?,
    })
}

fn decode_readable_rows(rows: Vec<ScheduledRunRow>) -> Vec<ScheduledRun> {
    rows.into_iter()
        .filter_map(|row| {
            let id = row.id;
            match decode_row(row) {
                Ok(run) => Some(run),
                Err(error) => {
                    tracing::warn!(run_id = id, %error, "skipping unreadable scheduled run");
                    None
                }
            }
        })
        .collect()
}

fn contains_non_finite_float(value: &Variant) -> bool {
    match value {
        Variant::Float(number) => !number.is_finite(),
        Variant::Array(items) => items.iter().any(contains_non_finite_float),
        Variant::Object(fields) => fields.values().any(contains_non_finite_float),
        _ => false,
    }
}

fn reject_non_finite_args(spec: &ScheduledRunSpec) -> Result<(), StorageError> {
    if spec.args.values().any(contains_non_finite_float) {
        return Err(StorageError::ValidationFailed {
            field: "args".to_owned(),
            reason: "non-finite float values cannot be stored".to_owned(),
        });
    }
    Ok(())
}

async fn supersede_pending_key(
    conn: &mut SqliteConnection,
    key: &str,
    at: OffsetDateTime,
) -> Result<Option<ScheduledRunId>, SqliteStorageError> {
    let superseded: Option<i64> = sqlx::query_scalar(
        "UPDATE scheduled_runs SET state = ?, outcome_reason = ?, resolved_at = ?
         WHERE key = ? AND state = ? RETURNING id",
    )
    .bind(STATE_CANCELLED)
    .bind(REASON_SUPERSEDED)
    .bind(to_epoch_ms(at))
    .bind(key)
    .bind(STATE_PENDING)
    .fetch_optional(conn)
    .await
    .map_err(SqliteStorageError::Sqlx)?;
    Ok(superseded.map(ScheduledRunId::new))
}

pub struct SqliteScheduledRunRepo {
    db: SqlitePools,
}

impl SqliteScheduledRunRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }

    async fn cancel_pending(
        &self,
        sql: &'static str,
        bind: Binding<'_>,
        at: OffsetDateTime,
    ) -> Result<bool, StorageError> {
        let query = sqlx::query(sql)
            .bind(STATE_CANCELLED)
            .bind(REASON_CANCELLED)
            .bind(to_epoch_ms(at));
        let query = match bind {
            Binding::Id(id) => query.bind(id.get()),
            Binding::Key(key) => query.bind(key.to_owned()),
        };
        let result = query
            .bind(STATE_PENDING)
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected() > 0)
    }
}

enum Binding<'a> {
    Id(ScheduledRunId),
    Key(&'a str),
}

const CANCEL_BY_ID_SQL: &str =
    "UPDATE scheduled_runs SET state = ?, outcome_reason = ?, resolved_at = ?
     WHERE id = ? AND state = ?";
const CANCEL_BY_KEY_SQL: &str =
    "UPDATE scheduled_runs SET state = ?, outcome_reason = ?, resolved_at = ?
     WHERE key = ? AND state = ?";

#[async_trait]
impl ScheduledRunRepo for SqliteScheduledRunRepo {
    async fn schedule(
        &self,
        spec: &ScheduledRunSpec,
    ) -> Result<ScheduledRunPlacement, StorageError> {
        reject_non_finite_args(spec)?;
        let (policy, skip_late_ms) = policy_columns(spec.missed_run_policy)?;
        let args = serde_json::to_string(&spec.args)?;
        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        let superseded = match spec.key.as_deref() {
            Some(key) => supersede_pending_key(&mut tx, key, spec.scheduled_at).await?,
            None => None,
        };

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO scheduled_runs (
                target_action_id, due_at, key, missed_policy, skip_late_ms, args,
                scheduled_by_action, scheduled_by_run, trigger_event_id, scheduled_at, label, state
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(spec.target_action_id.to_string())
        .bind(to_epoch_ms(spec.due_at))
        .bind(spec.key.as_deref())
        .bind(policy)
        .bind(skip_late_ms)
        .bind(args)
        .bind(spec.scheduled_by_action.map(|id| id.to_string()))
        .bind(spec.scheduled_by_run.as_deref())
        .bind(spec.trigger_event_id.map(|id| id.to_string()))
        .bind(to_epoch_ms(spec.scheduled_at))
        .bind(&spec.label)
        .bind(STATE_PENDING)
        .fetch_one(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(ScheduledRunPlacement {
            id: ScheduledRunId::new(id),
            superseded,
        })
    }

    async fn get(&self, id: ScheduledRunId) -> Result<Option<ScheduledRun>, StorageError> {
        let row = sqlx::query_as::<_, ScheduledRunRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM scheduled_runs WHERE id = ?"
        ))
        .bind(id.get())
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        row.map(decode_row).transpose()
    }

    async fn list_pending(&self) -> Result<Vec<ScheduledRun>, StorageError> {
        let rows = sqlx::query_as::<_, ScheduledRunRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM scheduled_runs WHERE state = ? ORDER BY due_at, id"
        ))
        .bind(STATE_PENDING)
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(decode_readable_rows(rows))
    }

    async fn list_due(&self, now: OffsetDateTime) -> Result<Vec<ScheduledRun>, StorageError> {
        let rows = sqlx::query_as::<_, ScheduledRunRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM scheduled_runs
             WHERE state = ? AND due_at <= ? ORDER BY due_at, id"
        ))
        .bind(STATE_PENDING)
        .bind(to_epoch_ms(now))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(decode_readable_rows(rows))
    }

    async fn fail_unreadable_due(
        &self,
        now: OffsetDateTime,
        reason: &str,
    ) -> Result<Vec<ScheduledRunId>, StorageError> {
        let rows = sqlx::query_as::<_, ScheduledRunRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM scheduled_runs
             WHERE state = ? AND due_at <= ? ORDER BY due_at, id"
        ))
        .bind(STATE_PENDING)
        .bind(to_epoch_ms(now))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        let unreadable: Vec<i64> = rows
            .into_iter()
            .filter_map(|row| {
                let id = row.id;
                decode_row(row).err().map(|_| id)
            })
            .collect();
        let mut failed = Vec::with_capacity(unreadable.len());
        for id in unreadable {
            let result = sqlx::query(
                "UPDATE scheduled_runs SET state = ?, outcome_reason = ?, resolved_at = ?
                 WHERE id = ? AND state = ?",
            )
            .bind(STATE_FAILED)
            .bind(reason)
            .bind(to_epoch_ms(now))
            .bind(id)
            .bind(STATE_PENDING)
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;
            if result.rows_affected() > 0 {
                failed.push(ScheduledRunId::new(id));
            }
        }
        Ok(failed)
    }

    async fn next_due(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        let due: Option<i64> =
            sqlx::query_scalar("SELECT MIN(due_at) FROM scheduled_runs WHERE state = ?")
                .bind(STATE_PENDING)
                .fetch_one(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;
        Ok(due.map(from_epoch_ms).transpose()?)
    }

    async fn claim(
        &self,
        id: ScheduledRunId,
        claimed_at: OffsetDateTime,
    ) -> Result<Option<ScheduledRun>, StorageError> {
        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        let row = sqlx::query_as::<_, ScheduledRunRow>(concat!(
            "UPDATE scheduled_runs SET state = ?, resolved_at = ?
             WHERE id = ? AND state = ? RETURNING ",
            columns!(),
            ""
        ))
        .bind(STATE_DISPATCHED)
        .bind(to_epoch_ms(claimed_at))
        .bind(id.get())
        .bind(STATE_PENDING)
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let run = decode_row(row)?;
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(Some(run))
    }

    async fn cancel(&self, id: ScheduledRunId, at: OffsetDateTime) -> Result<bool, StorageError> {
        self.cancel_pending(CANCEL_BY_ID_SQL, Binding::Id(id), at)
            .await
    }

    async fn cancel_by_key(&self, key: &str, at: OffsetDateTime) -> Result<bool, StorageError> {
        self.cancel_pending(CANCEL_BY_KEY_SQL, Binding::Key(key), at)
            .await
    }

    async fn settle(
        &self,
        id: ScheduledRunId,
        outcome: ScheduledRunOutcome,
        reason: Option<String>,
        at: OffsetDateTime,
    ) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE scheduled_runs SET state = ?, outcome_reason = ?, resolved_at = ?
             WHERE id = ? AND state IN (?, ?)",
        )
        .bind(state_label(outcome.into()))
        .bind(reason)
        .bind(to_epoch_ms(at))
        .bind(id.get())
        .bind(STATE_PENDING)
        .bind(STATE_DISPATCHED)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_recent_resolved(&self, limit: usize) -> Result<Vec<ScheduledRun>, StorageError> {
        let rows = sqlx::query_as::<_, ScheduledRunRow>(concat!(
            "SELECT ",
            columns!(),
            " FROM scheduled_runs
             WHERE state <> ? ORDER BY resolved_at DESC, id DESC LIMIT ?"
        ))
        .bind(STATE_PENDING)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(decode_readable_rows(rows))
    }

    async fn prune_resolved_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        let result = sqlx::query("DELETE FROM scheduled_runs WHERE state <> ? AND resolved_at < ?")
            .bind(STATE_PENDING)
            .bind(to_epoch_ms(cutoff))
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected())
    }

    async fn count_pending(&self) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM scheduled_runs WHERE state = ?")
            .bind(STATE_PENDING)
            .fetch_one(self.db.reader())
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(count.max(0) as u64)
    }

    async fn count_pending_for_action(&self, action_id: ActionId) -> Result<u64, StorageError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM scheduled_runs WHERE state = ? AND target_action_id = ?",
        )
        .bind(STATE_PENDING)
        .bind(action_id.to_string())
        .fetch_one(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(count.max(0) as u64)
    }
}
