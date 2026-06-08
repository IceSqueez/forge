#![allow(clippy::expect_used)]

use forge_storage_sqlite::apply_migrations;

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
                      'queues', 'actions', 'scripts', 'event_log',
                      'soundboard_clips', 'voice_aliases', 'ignore_profile',
                      'replacement_rules', 'viewers', 'action_executions',
                      'trigger_instances', 'action_trigger_instances')",
    )
    .fetch_one(&pool)
    .await
    .expect("query sqlite_master");

    assert_eq!(count, 17, "expected 17 tables after all migrations");
}
