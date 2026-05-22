#![allow(clippy::expect_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::{SqliteBackend, apply_migrations};

const TEST_KEY: [u8; 32] = [0xab; 32];

#[tokio::test]
async fn all_core_tables_exist_after_migration() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' \
         AND name IN ('globals', 'user_globals', 'settings', 'action_history', 'credentials',
                      'queues', 'actions', 'triggers', 'commands', 'scripts', 'event_log',
                      'soundboard_clips', 'voice_aliases', 'ignore_profile',
                      'replacement_rules', 'viewers', 'action_executions')",
    )
    .fetch_one(&pool)
    .await
    .expect("query sqlite_master");

    assert_eq!(count, 17, "expected 17 tables after all migrations");
}

#[tokio::test]
async fn schema_version_matches_migration_count() {
    let backend = SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open");
    let version = backend.schema_version().await.expect("schema_version");
    assert_eq!(
        version, 10,
        "schema_version must be 10 after migrations 0001 through 0010"
    );
}

#[tokio::test]
async fn default_queue_exists_after_migration() {
    let backend = SqliteBackend::open_with_key("sqlite::memory:", TEST_KEY)
        .await
        .expect("open");
    let got = backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .expect("get_by_name");
    assert!(
        got.is_some(),
        "default queue must be seeded by migration 0002"
    );
}
