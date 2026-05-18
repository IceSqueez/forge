use async_trait::async_trait;
use forge_storage::{ScriptRecord, ScriptRepo, StorageError};
use forge_types::{ScriptContract, ScriptId};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

fn hash_body(body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body.as_bytes());
    format!("{:x}", hasher.finalize())
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

type ScriptRow = (String, String, String, String, String, i64, i64, i64);

fn decode_row(
    (id_str, name, body, contract_json, body_hash, enabled, created_at_ms, last_modified_ms): ScriptRow,
) -> Result<ScriptRecord, SqliteStorageError> {
    Ok(ScriptRecord {
        id: parse_script_id(&id_str)?,
        name,
        body,
        contract: decode_contract(&contract_json)?,
        body_hash,
        enabled: enabled != 0,
        created_at: from_epoch_ms(created_at_ms)?,
        last_modified: from_epoch_ms(last_modified_ms)?,
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
        let row: Option<ScriptRow> = sqlx::query_as(&format!("{SELECT_COLS} WHERE id = ?"))
            .bind(&id_str)
            .fetch_optional(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        let row: Option<ScriptRow> = sqlx::query_as(&format!("{SELECT_COLS} WHERE name = ?"))
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
        let rows: Vec<ScriptRow> = sqlx::query_as(&format!("{SELECT_COLS} ORDER BY name"))
            .fetch_all(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        let rows: Vec<ScriptRow> =
            sqlx::query_as(&format!("{SELECT_COLS} WHERE enabled = 1 ORDER BY name"))
                .fetch_all(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|r| decode_row(r).map_err(StorageError::from))
            .collect()
    }
}
