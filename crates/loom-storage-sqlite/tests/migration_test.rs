#![allow(clippy::expect_used)]

use loom_storage_sqlite::apply_migrations;

#[tokio::test]
async fn all_alpha1_tables_exist_after_migration() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' \
         AND name IN ('globals', 'user_globals', 'settings', 'action_history', 'credentials')",
    )
    .fetch_one(&pool)
    .await
    .expect("query sqlite_master");

    assert_eq!(count, 5, "expected 5 tables after migration 0001");
}
