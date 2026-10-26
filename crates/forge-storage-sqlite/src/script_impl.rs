use std::str::FromStr;

use async_trait::async_trait;
use forge_storage::{
    ExecutionStatus, SCRIPT_RUN_NAMED_KIND, ScriptRecord, ScriptReferences, ScriptReferrer,
    ScriptRepo, ScriptTelemetry, StorageError,
};
use forge_types::{ActionId, ScriptContract, ScriptId, SubActionStep};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::epoch::{epoch_ms_now, from_epoch_ms};
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;
use crate::script_name_steps::{ScriptNameEdit, ScriptNameUses, apply_script_name_edit};

fn hash_body(body: &str) -> String {
    use std::fmt::Write as _;
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "writing to a String cannot fail"
        )]
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn parse_script_id(s: &str) -> Result<ScriptId, SqliteStorageError> {
    serde_json::from_str(&format!("\"{s}\""))
        .map_err(|e| SqliteStorageError::Decode(format!("invalid script id '{s}': {e}")))
}

fn decode_contract(json: &str) -> Result<ScriptContract, SqliteStorageError> {
    serde_json::from_str(json)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid contract json: {e}")))
}

#[derive(sqlx::FromRow)]
struct ScriptRow {
    id: String,
    name: String,
    body: String,
    contract_json: String,
    body_hash: String,
    enabled: i64,
    created_at: i64,
    last_modified: i64,
}

fn decode_row(row: ScriptRow) -> Result<ScriptRecord, SqliteStorageError> {
    Ok(ScriptRecord {
        id: parse_script_id(&row.id)?,
        name: row.name,
        body: row.body,
        contract: decode_contract(&row.contract_json)?,
        body_hash: row.body_hash,
        enabled: row.enabled != 0,
        created_at: from_epoch_ms(row.created_at)?,
        last_modified: from_epoch_ms(row.last_modified)?,
    })
}

const SELECT_COLS: &str = "SELECT id, name, body, contract_json, body_hash, enabled, created_at, last_modified \
     FROM scripts";

#[derive(sqlx::FromRow)]
struct CandidateActionRow {
    id: String,
    name: String,
    sub_actions: String,
    archived: i64,
}

struct ScannedAction {
    row: CandidateActionRow,
    steps: Vec<SubActionStep>,
    uses: ScriptNameUses,
}

const CANDIDATE_ACTIONS: &str = "SELECT id, name, sub_actions, archived_at IS NOT NULL AS archived \
     FROM actions WHERE instr(sub_actions, ?) > 0";

fn scan_action(
    row: CandidateActionRow,
    edit: &ScriptNameEdit<'_>,
) -> Result<ScannedAction, SqliteStorageError> {
    let mut steps: Vec<SubActionStep> = serde_json::from_str(&row.sub_actions)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid sub_actions json: {e}")))?;
    let uses = apply_script_name_edit(&mut steps, edit);
    Ok(ScannedAction { row, steps, uses })
}

fn collect_live_referrer(
    scanned: &ScannedAction,
    references: &mut ScriptReferences,
) -> Result<(), SqliteStorageError> {
    if scanned.row.archived != 0 {
        return Ok(());
    }
    let referrer = || -> Result<ScriptReferrer, SqliteStorageError> {
        Ok(ScriptReferrer {
            action_id: ActionId::from_str(&scanned.row.id).map_err(|e| {
                SqliteStorageError::Decode(format!("invalid action id '{}': {e}", scanned.row.id))
            })?,
            action_name: scanned.row.name.clone(),
        })
    };
    if scanned.uses.literal {
        references.literal.push(referrer()?);
    }
    if scanned.uses.templated {
        references.templated.push(referrer()?);
    }
    Ok(())
}

pub struct SqliteScriptRepo {
    db: SqlitePools,
}

impl SqliteScriptRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[async_trait]
impl ScriptRepo for SqliteScriptRepo {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        let id_str = id.to_string();
        let row: Option<ScriptRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_COLS} WHERE id = ? AND archived_at IS NULL"
        )))
        .bind(&id_str)
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        let row: Option<ScriptRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_COLS} WHERE name = ? AND archived_at IS NULL"
        )))
        .bind(name)
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn save(&self, record: ScriptRecord) -> Result<(), StorageError> {
        let id_str = record.id.to_string();
        let now_ms = epoch_ms_now();
        let enabled_int: i64 = i64::from(record.enabled);
        let computed_hash = hash_body(&record.body);
        let contract_json =
            serde_json::to_string(&record.contract).map_err(|e| StorageError::Connection {
                reason: e.to_string(),
            })?;

        sqlx::query(
            "INSERT INTO scripts \
             (id, name, body, contract_json, body_hash, enabled, created_at, last_modified) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
                 name          = excluded.name, \
                 body          = excluded.body, \
                 contract_json = excluded.contract_json, \
                 body_hash     = excluded.body_hash, \
                 enabled       = excluded.enabled, \
                 last_modified = ?",
        )
        .bind(&id_str)
        .bind(&record.name)
        .bind(&record.body)
        .bind(&contract_json)
        .bind(&computed_hash)
        .bind(enabled_int)
        .bind(now_ms)
        .bind(now_ms)
        .bind(now_ms)
        .execute(self.db.writer())
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
                StorageError::NameCollision {
                    name: record.name.clone(),
                }
            }
            other => SqliteStorageError::Sqlx(other).into(),
        })?;

        Ok(())
    }

    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM scripts WHERE id = ?")
            .bind(&id_str)
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let rows: Vec<ScriptRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_COLS} WHERE archived_at IS NULL ORDER BY name"
        )))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let rows: Vec<ScriptRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_COLS} WHERE enabled = 1 AND archived_at IS NULL ORDER BY name"
        )))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn record_execution(
        &self,
        script_id: ScriptId,
        started_at: OffsetDateTime,
        duration_ms: u64,
        status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        let id_str = script_id.to_string();
        let started_at_secs = started_at.unix_timestamp();
        let duration_i64 = duration_ms as i64;
        let status_str = match status {
            ExecutionStatus::Success => "ok",
            ExecutionStatus::Error => "err",
        };
        sqlx::query(
            "INSERT INTO script_executions (script_id, started_at, duration_ms, status)
             VALUES (?, ?, ?, ?)",
        )
        .bind(id_str)
        .bind(started_at_secs)
        .bind(duration_i64)
        .bind(status_str)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn telemetry(&self, id: ScriptId) -> Result<ScriptTelemetry, StorageError> {
        let id_str = id.to_string();
        let now = OffsetDateTime::now_utc();
        let start_of_today = now.replace_time(time::Time::MIDNIGHT).unix_timestamp();

        #[derive(sqlx::FromRow)]
        struct TelemetryRow {
            last_run: Option<i64>,
            runs_today: i64,
            avg_duration_ms: Option<f64>,
        }

        let row: TelemetryRow = sqlx::query_as(
            "WITH \
               lr AS (SELECT MAX(started_at) AS v \
                      FROM script_executions WHERE script_id = ?), \
               rt AS (SELECT COUNT(*) AS v \
                      FROM script_executions \
                      WHERE script_id = ? AND started_at >= ?), \
               ad AS (SELECT AVG(duration_ms) AS v \
                      FROM (SELECT duration_ms FROM script_executions \
                            WHERE script_id = ? ORDER BY started_at DESC LIMIT 100)) \
             SELECT lr.v AS last_run, rt.v AS runs_today, ad.v AS avg_duration_ms \
             FROM lr, rt, ad",
        )
        .bind(&id_str)
        .bind(&id_str)
        .bind(start_of_today)
        .bind(&id_str)
        .fetch_one(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(ScriptTelemetry {
            last_run: row
                .last_run
                .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok()),
            runs_today: row.runs_today.max(0) as u64,
            avg_duration_ms: row.avg_duration_ms.map(|v| v.round() as u64),
        })
    }

    async fn prune_executions_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        let cutoff_secs = cutoff.unix_timestamp();
        let result = sqlx::query("DELETE FROM script_executions WHERE started_at < ?")
            .bind(cutoff_secs)
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected())
    }

    async fn rename_cascading(
        &self,
        id: ScriptId,
        new_name: &str,
    ) -> Result<ScriptReferences, StorageError> {
        if new_name.trim().is_empty() {
            return Err(StorageError::ValidationFailed {
                field: "name".to_owned(),
                reason: "must not be empty".to_owned(),
            });
        }
        let id_str = id.to_string();
        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        let old_name: Option<(String,)> =
            sqlx::query_as("SELECT name FROM scripts WHERE id = ? AND archived_at IS NULL")
                .bind(&id_str)
                .fetch_optional(&mut *tx)
                .await
                .map_err(SqliteStorageError::Sqlx)?;
        let Some((old_name,)) = old_name else {
            return Err(StorageError::NotFound { key: id_str });
        };
        if old_name == new_name {
            return Ok(ScriptReferences::default());
        }

        let collision: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM scripts WHERE name = ? AND id != ?")
                .bind(new_name)
                .bind(&id_str)
                .fetch_optional(&mut *tx)
                .await
                .map_err(SqliteStorageError::Sqlx)?;
        if collision.is_some() {
            return Err(StorageError::NameCollision {
                name: new_name.to_owned(),
            });
        }

        sqlx::query("UPDATE scripts SET name = ?, last_modified = ? WHERE id = ?")
            .bind(new_name)
            .bind(epoch_ms_now())
            .bind(&id_str)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        let candidates: Vec<CandidateActionRow> = sqlx::query_as(CANDIDATE_ACTIONS)
            .bind(SCRIPT_RUN_NAMED_KIND)
            .fetch_all(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        let edit = ScriptNameEdit {
            old_name: &old_name,
            new_name: Some(new_name),
        };
        let mut references = ScriptReferences::default();
        for candidate in candidates {
            let scanned = scan_action(candidate, &edit)?;
            if scanned.uses.literal {
                let sub_actions_json =
                    serde_json::to_string(&scanned.steps).map_err(StorageError::Serialization)?;
                sqlx::query("UPDATE actions SET sub_actions = ? WHERE id = ?")
                    .bind(&sub_actions_json)
                    .bind(&scanned.row.id)
                    .execute(&mut *tx)
                    .await
                    .map_err(SqliteStorageError::Sqlx)?;
            }
            collect_live_referrer(&scanned, &mut references)?;
        }

        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(references)
    }

    async fn referrers(&self, name: &str) -> Result<ScriptReferences, StorageError> {
        let candidates: Vec<CandidateActionRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{CANDIDATE_ACTIONS} AND archived_at IS NULL"
        )))
        .bind(SCRIPT_RUN_NAMED_KIND)
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        let edit = ScriptNameEdit {
            old_name: name,
            new_name: None,
        };
        let mut references = ScriptReferences::default();
        for candidate in candidates {
            collect_live_referrer(&scan_action(candidate, &edit)?, &mut references)?;
        }
        Ok(references)
    }

    async fn archive_script(&self, id: ScriptId) -> Result<bool, StorageError> {
        let result =
            sqlx::query("UPDATE scripts SET archived_at = ? WHERE id = ? AND archived_at IS NULL")
                .bind(epoch_ms_now())
                .bind(id.to_string())
                .execute(self.db.writer())
                .await
                .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected() > 0)
    }

    async fn restore_script(&self, id: ScriptId) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "UPDATE scripts SET archived_at = NULL WHERE id = ? AND archived_at IS NOT NULL",
        )
        .bind(id.to_string())
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected() > 0)
    }

    async fn list_archived_scripts(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let rows: Vec<ScriptRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_COLS} WHERE archived_at IS NOT NULL ORDER BY name"
        )))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }
}
