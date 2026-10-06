use async_trait::async_trait;
use forge_storage::{
    BAN_LEDGER_LIST_CAP, BanLedgerEntry, BanLedgerKey, BanLedgerRepo, BanOrigin, StorageError,
    ViewerPlatform, merge_ban_entry,
};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

const NANOS_PER_MILLI: i128 = 1_000_000;

macro_rules! entry_columns {
    () => {
        "platform, channel_id, viewer_id, viewer_name, reason, moderator, \
         banned_at, expires_at, platform_ban_id, origin"
    };
}

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / NANOS_PER_MILLI) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * NANOS_PER_MILLI)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch {ms}: {e}")))
}

fn at_stored_precision(dt: OffsetDateTime) -> Result<OffsetDateTime, SqliteStorageError> {
    from_epoch_ms(to_epoch_ms(dt))
}

#[derive(sqlx::FromRow)]
struct BanLedgerRow {
    platform: String,
    channel_id: String,
    viewer_id: String,
    viewer_name: String,
    reason: Option<String>,
    moderator: Option<String>,
    banned_at: i64,
    expires_at: Option<i64>,
    platform_ban_id: Option<String>,
    origin: String,
}

fn decode_row(row: BanLedgerRow) -> Result<BanLedgerEntry, SqliteStorageError> {
    let platform = ViewerPlatform::parse(&row.platform).ok_or_else(|| {
        SqliteStorageError::Decode(format!("invalid ban ledger platform '{}'", row.platform))
    })?;
    let origin = BanOrigin::parse(&row.origin).ok_or_else(|| {
        SqliteStorageError::Decode(format!("invalid ban ledger origin '{}'", row.origin))
    })?;
    Ok(BanLedgerEntry {
        key: BanLedgerKey {
            platform,
            channel_id: row.channel_id,
            viewer_id: row.viewer_id,
        },
        viewer_name: row.viewer_name,
        reason: row.reason,
        moderator: row.moderator,
        banned_at: from_epoch_ms(row.banned_at)?,
        expires_at: row.expires_at.map(from_epoch_ms).transpose()?,
        platform_ban_id: row.platform_ban_id,
        origin,
    })
}

fn normalized(entry: &BanLedgerEntry) -> Result<BanLedgerEntry, SqliteStorageError> {
    Ok(BanLedgerEntry {
        banned_at: at_stored_precision(entry.banned_at)?,
        expires_at: entry.expires_at.map(at_stored_precision).transpose()?,
        ..entry.clone()
    })
}

pub struct SqliteBanLedgerRepo {
    db: SqlitePools,
}

impl SqliteBanLedgerRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[async_trait]
impl BanLedgerRepo for SqliteBanLedgerRepo {
    async fn upsert(
        &self,
        entry: &BanLedgerEntry,
        now: OffsetDateTime,
    ) -> Result<BanLedgerEntry, StorageError> {
        let incoming = normalized(entry)?;
        let now_ms = to_epoch_ms(now);
        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        sqlx::query("DELETE FROM ban_ledger WHERE expires_at IS NOT NULL AND expires_at <= ?")
            .bind(now_ms)
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        let stored = sqlx::query_as::<_, BanLedgerRow>(concat!(
            "SELECT ",
            entry_columns!(),
            " FROM ban_ledger
             WHERE platform = ? AND channel_id = ? AND viewer_id = ?"
        ))
        .bind(incoming.key.platform.as_str())
        .bind(&incoming.key.channel_id)
        .bind(&incoming.key.viewer_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?
        .map(decode_row)
        .transpose()?;

        let merged = merge_ban_entry(stored, incoming, now);

        sqlx::query(concat!(
            "INSERT OR REPLACE INTO ban_ledger (",
            entry_columns!(),
            ")
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(merged.key.platform.as_str())
        .bind(&merged.key.channel_id)
        .bind(&merged.key.viewer_id)
        .bind(&merged.viewer_name)
        .bind(&merged.reason)
        .bind(&merged.moderator)
        .bind(to_epoch_ms(merged.banned_at))
        .bind(merged.expires_at.map(to_epoch_ms))
        .bind(&merged.platform_ban_id)
        .bind(merged.origin.as_str())
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(merged)
    }

    async fn remove(&self, key: &BanLedgerKey) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "DELETE FROM ban_ledger WHERE platform = ? AND channel_id = ? AND viewer_id = ?",
        )
        .bind(key.platform.as_str())
        .bind(&key.channel_id)
        .bind(&key.viewer_id)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn get(
        &self,
        key: &BanLedgerKey,
        now: OffsetDateTime,
    ) -> Result<Option<BanLedgerEntry>, StorageError> {
        let row = sqlx::query_as::<_, BanLedgerRow>(concat!(
            "SELECT ",
            entry_columns!(),
            " FROM ban_ledger
             WHERE platform = ? AND channel_id = ? AND viewer_id = ?
               AND (expires_at IS NULL OR expires_at > ?)"
        ))
        .bind(key.platform.as_str())
        .bind(&key.channel_id)
        .bind(&key.viewer_id)
        .bind(to_epoch_ms(now))
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(row.map(decode_row).transpose()?)
    }

    async fn list_active(
        &self,
        platform: ViewerPlatform,
        channel_id: &str,
        now: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<BanLedgerEntry>, StorageError> {
        let capped = i64::try_from(limit.min(BAN_LEDGER_LIST_CAP)).unwrap_or(i64::MAX);
        let rows = sqlx::query_as::<_, BanLedgerRow>(concat!(
            "SELECT ",
            entry_columns!(),
            " FROM ban_ledger
             WHERE platform = ? AND channel_id = ?
               AND (expires_at IS NULL OR expires_at > ?)
             ORDER BY banned_at DESC, viewer_id
             LIMIT ?"
        ))
        .bind(platform.as_str())
        .bind(channel_id)
        .bind(to_epoch_ms(now))
        .bind(capped)
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }
}
