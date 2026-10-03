use async_trait::async_trait;
use forge_storage::{LatestRecord, LatestValueRepo, StorageError};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

const NANOS_PER_MILLI: i128 = 1_000_000;

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / NANOS_PER_MILLI) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * NANOS_PER_MILLI)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch {ms}: {e}")))
}

#[derive(sqlx::FromRow)]
struct LatestRow {
    slot: String,
    platform: String,
    payload: String,
    occurred_at: i64,
    updated_at: i64,
}

fn decode_row(row: LatestRow) -> Result<LatestRecord, StorageError> {
    Ok(LatestRecord {
        slot: row.slot,
        platform: row.platform,
        payload: serde_json::from_str(&row.payload)?,
        occurred_at: from_epoch_ms(row.occurred_at)?,
        updated_at: from_epoch_ms(row.updated_at)?,
    })
}

pub struct SqliteLatestValueRepo {
    db: SqlitePools,
}

impl SqliteLatestValueRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[async_trait]
impl LatestValueRepo for SqliteLatestValueRepo {
    async fn upsert_if_newer(&self, record: &LatestRecord) -> Result<bool, StorageError> {
        let payload = serde_json::to_string(&record.payload)?;
        let result = sqlx::query(
            "INSERT INTO latest_values (slot, platform, payload, occurred_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(slot, platform) DO UPDATE SET
                payload = excluded.payload,
                occurred_at = excluded.occurred_at,
                updated_at = excluded.updated_at
             WHERE excluded.occurred_at > latest_values.occurred_at",
        )
        .bind(&record.slot)
        .bind(&record.platform)
        .bind(payload)
        .bind(to_epoch_ms(record.occurred_at))
        .bind(to_epoch_ms(record.updated_at))
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_slot(&self, slot: &str) -> Result<Vec<LatestRecord>, StorageError> {
        let rows = sqlx::query_as::<_, LatestRow>(
            "SELECT slot, platform, payload, occurred_at, updated_at
             FROM latest_values WHERE slot = ? ORDER BY occurred_at DESC, platform",
        )
        .bind(slot)
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter().map(decode_row).collect()
    }

    async fn reset_slot(&self, slot: &str) -> Result<u64, StorageError> {
        let result = sqlx::query("DELETE FROM latest_values WHERE slot = ?")
            .bind(slot)
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }
}
