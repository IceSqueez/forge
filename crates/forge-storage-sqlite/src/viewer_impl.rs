use std::collections::HashMap;

use async_trait::async_trait;
use forge_storage::{StorageError, Viewer, ViewerMessage, ViewerPlatform, ViewerRepo};
use time::OffsetDateTime;

use crate::batch::insert_rows;
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, StorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000).map_err(|e| {
        StorageError::from(SqliteStorageError::Decode(format!(
            "invalid epoch {ms}: {e}"
        )))
    })
}

const VIEWER_COLUMNS: usize = 7;

struct ViewerTally<'a> {
    platform: &'static str,
    viewer_id: &'a str,
    username: &'a str,
    messages: i64,
}

pub struct SqliteViewerRepo {
    db: SqlitePools,
}

impl SqliteViewerRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[derive(sqlx::FromRow)]
struct ViewerRow {
    platform: String,
    viewer_id: String,
    username: String,
    first_seen_at: i64,
    last_seen_at: i64,
    message_count: i64,
    custom_greeting: i64,
}

fn row_to_viewer(row: ViewerRow) -> Result<Viewer, StorageError> {
    let platform = ViewerPlatform::parse(&row.platform).ok_or_else(|| {
        StorageError::from(SqliteStorageError::Decode(format!(
            "unknown platform `{}`",
            row.platform
        )))
    })?;
    Ok(Viewer {
        viewer_id: row.viewer_id,
        platform,
        username: row.username,
        first_seen_at: from_epoch_ms(row.first_seen_at)?,
        last_seen_at: from_epoch_ms(row.last_seen_at)?,
        message_count: u64::try_from(row.message_count).unwrap_or(0),
        custom_greeting: row.custom_greeting != 0,
    })
}

#[async_trait]
impl ViewerRepo for SqliteViewerRepo {
    async fn list(&self) -> Result<Vec<Viewer>, StorageError> {
        let rows: Vec<ViewerRow> = sqlx::query_as(
            "SELECT platform, viewer_id, username, first_seen_at, last_seen_at,
                    message_count, custom_greeting
             FROM viewers
             ORDER BY last_seen_at DESC",
        )
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter().map(row_to_viewer).collect()
    }

    async fn count(&self) -> Result<u64, StorageError> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM viewers")
            .fetch_one(self.db.reader())
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    async fn get(
        &self,
        platform: ViewerPlatform,
        viewer_id: &str,
    ) -> Result<Option<Viewer>, StorageError> {
        let row: Option<ViewerRow> = sqlx::query_as(
            "SELECT platform, viewer_id, username, first_seen_at, last_seen_at,
                    message_count, custom_greeting
             FROM viewers WHERE platform = ? AND viewer_id = ?",
        )
        .bind(platform.as_str())
        .bind(viewer_id)
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(row_to_viewer).transpose()
    }

    async fn record_message(
        &self,
        platform: ViewerPlatform,
        viewer_id: &str,
        username: &str,
    ) -> Result<(), StorageError> {
        let now_ms = to_epoch_ms(OffsetDateTime::now_utc());
        sqlx::query(
            "INSERT INTO viewers
                (platform, viewer_id, username, first_seen_at, last_seen_at, message_count, custom_greeting)
             VALUES (?, ?, ?, ?, ?, 1, 0)
             ON CONFLICT(platform, viewer_id) DO UPDATE SET
                username      = excluded.username,
                last_seen_at  = excluded.last_seen_at,
                message_count = message_count + 1",
        )
        .bind(platform.as_str())
        .bind(viewer_id)
        .bind(username)
        .bind(now_ms)
        .bind(now_ms)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn record_messages(&self, messages: &[ViewerMessage]) -> Result<(), StorageError> {
        if messages.is_empty() {
            return Ok(());
        }
        let now_ms = to_epoch_ms(OffsetDateTime::now_utc());
        let mut tallies: Vec<ViewerTally<'_>> = Vec::new();
        let mut positions: HashMap<(&str, &str), usize> = HashMap::new();
        for message in messages {
            let key = (message.platform.as_str(), message.viewer_id.as_str());
            match positions.get(&key) {
                Some(&position) => {
                    let tally = &mut tallies[position];
                    tally.username = &message.username;
                    tally.messages += 1;
                }
                None => {
                    positions.insert(key, tallies.len());
                    tallies.push(ViewerTally {
                        platform: key.0,
                        viewer_id: key.1,
                        username: &message.username,
                        messages: 1,
                    });
                }
            }
        }

        let mut tx = self
            .db
            .writer()
            .begin()
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        insert_rows(
            &mut tx,
            "INSERT INTO viewers
                (platform, viewer_id, username, first_seen_at, last_seen_at, message_count, custom_greeting) ",
            VIEWER_COLUMNS,
            " ON CONFLICT(platform, viewer_id) DO UPDATE SET
                username      = excluded.username,
                last_seen_at  = excluded.last_seen_at,
                message_count = message_count + excluded.message_count",
            &tallies,
            |values, tally| {
                values
                    .push_bind(tally.platform)
                    .push_bind(tally.viewer_id)
                    .push_bind(tally.username)
                    .push_bind(now_ms)
                    .push_bind(now_ms)
                    .push_bind(tally.messages)
                    .push_bind(0_i64);
            },
        )
        .await?;
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    async fn set_custom_greeting(
        &self,
        platform: ViewerPlatform,
        viewer_id: &str,
        enabled: bool,
    ) -> Result<bool, StorageError> {
        let flag = if enabled { 1 } else { 0 };
        let result = sqlx::query(
            "UPDATE viewers SET custom_greeting = ?
             WHERE platform = ? AND viewer_id = ?",
        )
        .bind(flag)
        .bind(platform.as_str())
        .bind(viewer_id)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::SqliteBackend;
    use crate::test_support::{Sandboxed, sandboxed_backend};
    use forge_storage::ViewerPlatform;

    async fn open() -> Sandboxed<SqliteBackend> {
        sandboxed_backend([0xab; 32]).await
    }

    #[tokio::test]
    async fn record_message_creates_viewer() {
        let backend = open().await;
        let repo = backend.viewer_repo_impl();
        repo.record_message(ViewerPlatform::Twitch, "u1", "Alice")
            .await
            .unwrap();
        let v = repo
            .get(ViewerPlatform::Twitch, "u1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v.username, "Alice");
        assert_eq!(v.message_count, 1);
        assert!(!v.custom_greeting);
    }

    #[tokio::test]
    async fn record_message_increments_count() {
        let backend = open().await;
        let repo = backend.viewer_repo_impl();
        for _ in 0..3 {
            repo.record_message(ViewerPlatform::Twitch, "u1", "Alice")
                .await
                .unwrap();
        }
        let v = repo
            .get(ViewerPlatform::Twitch, "u1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v.message_count, 3);
    }

    #[tokio::test]
    async fn record_message_updates_username() {
        let backend = open().await;
        let repo = backend.viewer_repo_impl();
        repo.record_message(ViewerPlatform::Twitch, "u1", "Old")
            .await
            .unwrap();
        repo.record_message(ViewerPlatform::Twitch, "u1", "New")
            .await
            .unwrap();
        let v = repo
            .get(ViewerPlatform::Twitch, "u1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v.username, "New");
    }

    #[tokio::test]
    async fn list_orders_by_last_seen_desc() {
        let backend = open().await;
        let repo = backend.viewer_repo_impl();
        repo.record_message(ViewerPlatform::Twitch, "u1", "A")
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        repo.record_message(ViewerPlatform::Twitch, "u2", "B")
            .await
            .unwrap();
        let listed = repo.list().await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].viewer_id, "u2");
    }

    #[tokio::test]
    async fn set_custom_greeting_toggles_flag() {
        let backend = open().await;
        let repo = backend.viewer_repo_impl();
        repo.record_message(ViewerPlatform::Twitch, "u1", "A")
            .await
            .unwrap();
        assert!(
            repo.set_custom_greeting(ViewerPlatform::Twitch, "u1", true)
                .await
                .unwrap()
        );
        let v = repo
            .get(ViewerPlatform::Twitch, "u1")
            .await
            .unwrap()
            .unwrap();
        assert!(v.custom_greeting);
    }

    #[tokio::test]
    async fn set_custom_greeting_returns_false_for_missing_viewer() {
        let backend = open().await;
        let repo = backend.viewer_repo_impl();
        assert!(
            !repo
                .set_custom_greeting(ViewerPlatform::Twitch, "ghost", true)
                .await
                .unwrap()
        );
    }
}
