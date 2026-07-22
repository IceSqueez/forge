use async_trait::async_trait;
use forge_storage::{StorageError, Viewer, ViewerPlatform, ViewerRepo};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

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

pub struct SqliteViewerRepo {
    pool: sqlx::SqlitePool,
}

impl SqliteViewerRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
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
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter().map(row_to_viewer).collect()
    }

    async fn count(&self) -> Result<u64, StorageError> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM viewers")
            .fetch_one(&self.pool)
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
        .fetch_optional(&self.pool)
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
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
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
        .execute(&self.pool)
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
    use forge_storage::ViewerPlatform;

    async fn open() -> SqliteBackend {
        SqliteBackend::open_with_key(":memory:", [0xab; 32])
            .await
            .expect("open")
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
