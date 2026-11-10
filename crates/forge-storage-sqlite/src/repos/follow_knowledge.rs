use async_trait::async_trait;
use forge_storage::{
    FollowKnowledgeEntry, FollowKnowledgeKey, FollowKnowledgeRepo, KnownFollow, StorageError,
};
use forge_types::PlatformId;
use time::OffsetDateTime;

use crate::batch::insert_rows;
use crate::codec::epoch::{from_epoch_ms, to_epoch_ms};
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

const DELETE_CHUNK_ROWS: i64 = 500;
const ENTRY_COLUMN_COUNT: usize = 6;

const STATUS_FOLLOWING: &str = "following";
const STATUS_NOT_FOLLOWING: &str = "not_following";
const STATUS_HIDDEN: &str = "hidden";

macro_rules! entry_columns {
    () => {
        "platform, viewer_id, status, followed_at, observed_at, fresh_until"
    };
}

macro_rules! entry_update_assignments {
    () => {
        " ON CONFLICT(platform, viewer_id) DO UPDATE SET
             status = excluded.status,
             followed_at = excluded.followed_at,
             observed_at = excluded.observed_at,
             fresh_until = excluded.fresh_until"
    };
}

#[derive(sqlx::FromRow)]
struct FollowKnowledgeRow {
    platform: String,
    viewer_id: String,
    status: String,
    followed_at: Option<i64>,
    observed_at: i64,
    fresh_until: i64,
}

struct EncodedEntry<'a> {
    platform: &'static str,
    viewer_id: &'a str,
    status: &'static str,
    followed_at: Option<i64>,
    observed_at: i64,
    fresh_until: i64,
}

fn encode(entry: &FollowKnowledgeEntry) -> EncodedEntry<'_> {
    let (status, followed_at) = match entry.follow {
        KnownFollow::FollowingSince(since) => (STATUS_FOLLOWING, Some(to_epoch_ms(since))),
        KnownFollow::NotFollowing => (STATUS_NOT_FOLLOWING, None),
        KnownFollow::Hidden => (STATUS_HIDDEN, None),
    };
    EncodedEntry {
        platform: entry.key.platform.as_str(),
        viewer_id: &entry.key.viewer_id,
        status,
        followed_at,
        observed_at: to_epoch_ms(entry.observed_at),
        fresh_until: to_epoch_ms(entry.fresh_until),
    }
}

fn decode(row: FollowKnowledgeRow) -> Result<FollowKnowledgeEntry, SqliteStorageError> {
    let platform = PlatformId::from_canonical(&row.platform).ok_or_else(|| {
        SqliteStorageError::Decode(format!(
            "invalid follow knowledge platform '{}'",
            row.platform
        ))
    })?;
    let follow = match (row.status.as_str(), row.followed_at) {
        (STATUS_FOLLOWING, Some(since)) => KnownFollow::FollowingSince(from_epoch_ms(since)?),
        (STATUS_NOT_FOLLOWING, None) => KnownFollow::NotFollowing,
        (STATUS_HIDDEN, None) => KnownFollow::Hidden,
        (status, _) => {
            return Err(SqliteStorageError::Decode(format!(
                "invalid follow knowledge status '{status}'"
            )));
        }
    };
    Ok(FollowKnowledgeEntry {
        key: FollowKnowledgeKey {
            platform,
            viewer_id: row.viewer_id,
        },
        follow,
        observed_at: from_epoch_ms(row.observed_at)?,
        fresh_until: from_epoch_ms(row.fresh_until)?,
    })
}

pub struct SqliteFollowKnowledgeRepo {
    db: SqlitePools,
}

impl SqliteFollowKnowledgeRepo {
    pub fn new(db: SqlitePools) -> Self {
        Self { db }
    }

    async fn delete_in_chunks<T>(
        &self,
        statement: &'static str,
        filter: T,
    ) -> Result<u64, SqliteStorageError>
    where
        T: for<'q> sqlx::Encode<'q, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Copy + Send,
    {
        let mut removed = 0;
        loop {
            let chunk = sqlx::query(statement)
                .bind(filter)
                .bind(DELETE_CHUNK_ROWS)
                .execute(self.db.writer())
                .await?
                .rows_affected();
            removed += chunk;
            if chunk < DELETE_CHUNK_ROWS.unsigned_abs() {
                return Ok(removed);
            }
        }
    }
}

#[async_trait]
impl FollowKnowledgeRepo for SqliteFollowKnowledgeRepo {
    async fn get(
        &self,
        key: &FollowKnowledgeKey,
    ) -> Result<Option<FollowKnowledgeEntry>, StorageError> {
        let row = sqlx::query_as::<_, FollowKnowledgeRow>(concat!(
            "SELECT ",
            entry_columns!(),
            " FROM follow_knowledge WHERE platform = ? AND viewer_id = ?"
        ))
        .bind(key.platform.as_str())
        .bind(&key.viewer_id)
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(row.map(decode).transpose()?)
    }

    async fn upsert(&self, entry: &FollowKnowledgeEntry) -> Result<(), StorageError> {
        let row = encode(entry);
        sqlx::query(concat!(
            "INSERT INTO follow_knowledge (",
            entry_columns!(),
            ") VALUES (?, ?, ?, ?, ?, ?)",
            entry_update_assignments!()
        ))
        .bind(row.platform)
        .bind(row.viewer_id)
        .bind(row.status)
        .bind(row.followed_at)
        .bind(row.observed_at)
        .bind(row.fresh_until)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn seed(&self, entries: &[FollowKnowledgeEntry]) -> Result<(), StorageError> {
        let rows: Vec<EncodedEntry<'_>> = entries.iter().map(encode).collect();
        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        insert_rows(
            &mut tx,
            concat!("INSERT INTO follow_knowledge (", entry_columns!(), ") "),
            ENTRY_COLUMN_COUNT,
            concat!(
                entry_update_assignments!(),
                " WHERE excluded.observed_at > follow_knowledge.observed_at"
            ),
            &rows,
            |separated, row| {
                separated
                    .push_bind(row.platform)
                    .push_bind(row.viewer_id.to_owned())
                    .push_bind(row.status)
                    .push_bind(row.followed_at)
                    .push_bind(row.observed_at)
                    .push_bind(row.fresh_until);
            },
        )
        .await?;

        sqlx::query("UPDATE follow_knowledge_meta SET seeded = 1 WHERE id = 1")
            .execute(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn is_seeded(&self) -> Result<bool, StorageError> {
        let seeded: Option<bool> =
            sqlx::query_scalar("SELECT seeded FROM follow_knowledge_meta WHERE id = 1")
                .fetch_optional(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;
        Ok(seeded.unwrap_or(false))
    }

    async fn prune_expired(&self, now: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(self
            .delete_in_chunks(
                "DELETE FROM follow_knowledge WHERE rowid IN (
                     SELECT rowid FROM follow_knowledge WHERE fresh_until <= ? LIMIT ?
                 )",
                to_epoch_ms(now),
            )
            .await?)
    }

    async fn clear_platform(&self, platform: PlatformId) -> Result<u64, StorageError> {
        Ok(self
            .delete_in_chunks(
                "DELETE FROM follow_knowledge WHERE rowid IN (
                     SELECT rowid FROM follow_knowledge WHERE platform = ? LIMIT ?
                 )",
                platform.as_str(),
            )
            .await?)
    }
}
