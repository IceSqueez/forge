use async_trait::async_trait;
use forge_storage::{ExecutionStatus, ScriptRecord, ScriptRepo, ScriptTelemetry, StorageError};
use forge_types::{ScriptContract, ScriptId};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn hash_body(body: &str) -> String {
    use std::fmt::Write as _;
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest.iter() {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn epoch_ms_now() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
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

pub struct SqliteScriptRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteScriptRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ScriptRepo for SqliteScriptRepo {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        let id_str = id.to_string();
        let row: Option<ScriptRow> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT_COLS} WHERE id = ?")))
                .bind(&id_str)
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        let row: Option<ScriptRow> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT_COLS} WHERE name = ?")))
                .bind(name)
                .fetch_optional(&self.pool)
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
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM scripts WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let rows: Vec<ScriptRow> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT_COLS} ORDER BY name")))
                .fetch_all(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let rows: Vec<ScriptRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT_COLS} WHERE enabled = 1 ORDER BY name"
        )))
        .fetch_all(&self.pool)
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
        .execute(&self.pool)
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
        .fetch_one(&self.pool)
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
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected())
    }
}
