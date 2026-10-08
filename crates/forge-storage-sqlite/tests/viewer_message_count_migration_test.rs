#![allow(clippy::expect_used)]

use forge_storage::{ViewerMessage, ViewerPlatform, ViewerRepo};
use forge_storage_sqlite::{MIGRATIONS, SqliteViewerRepo, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_DROP: i64 = 52;
const FIRST_SEEN_MS: i64 = 1_700_000_000_000;
const LAST_SEEN_MS: i64 = 1_700_000_500_000;

async fn migrated_from_previous_schema() -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_DROP, &pool)
        .await
        .expect("the schema before the drop");
    sqlx::query(
        "INSERT INTO viewers
            (platform, viewer_id, username, first_seen_at, last_seen_at, message_count, custom_greeting)
         VALUES ('twitch', '42', 'ann', ?, ?, 17, 1)",
    )
    .bind(FIRST_SEEN_MS)
    .bind(LAST_SEEN_MS)
    .execute(&pool)
    .await
    .expect("a viewer row with a counter");
    apply_migrations(&pool).await.expect("migrate");
    (pool, dir)
}

#[tokio::test]
async fn dropping_the_counter_removes_the_column_and_keeps_every_other_field() {
    let (pool, _dir) = migrated_from_previous_schema().await;

    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('viewers')")
        .fetch_all(&pool)
        .await
        .expect("columns");
    let viewer = SqliteViewerRepo::new(pool)
        .get(ViewerPlatform::Twitch, "42")
        .await
        .expect("read")
        .expect("the existing viewer");

    assert_eq!(
        (
            columns.iter().any(|name| name == "message_count"),
            viewer.username.as_str(),
            viewer.first_seen_at.unix_timestamp(),
            viewer.last_seen_at.unix_timestamp(),
            viewer.custom_greeting,
        ),
        (
            false,
            "ann",
            FIRST_SEEN_MS / 1000,
            LAST_SEEN_MS / 1000,
            true
        )
    );
}

#[tokio::test]
async fn recording_messages_after_the_drop_updates_name_and_last_seen_but_not_first_seen() {
    let (pool, _dir) = migrated_from_previous_schema().await;
    let repo = SqliteViewerRepo::new(pool);

    repo.record_messages(&[ViewerMessage {
        platform: ViewerPlatform::Twitch,
        viewer_id: "42".to_owned(),
        username: "ann_renamed".to_owned(),
    }])
    .await
    .expect("record");

    let viewer = repo
        .get(ViewerPlatform::Twitch, "42")
        .await
        .expect("read")
        .expect("the viewer");
    assert_eq!(
        (
            viewer.username.as_str(),
            viewer.first_seen_at.unix_timestamp(),
            viewer.last_seen_at.unix_timestamp() > LAST_SEEN_MS / 1000,
        ),
        ("ann_renamed", FIRST_SEEN_MS / 1000, true)
    );
}
