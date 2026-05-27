#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage_sqlite::{apply_migrations, connect};

#[tokio::test]
async fn trigger_instances_tables_exist_after_migration_0012() {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' \
         AND name IN ('trigger_instances', 'action_trigger_instances')",
    )
    .fetch_one(&pool)
    .await
    .expect("query sqlite_master");

    assert_eq!(count, 2);
}

#[tokio::test]
async fn trigger_instances_insert_and_retrieve() {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");

    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined) \
         VALUES ('inst-001', 'twitch.chat.command', 'Test Trigger', '{}', 1, 1)",
    )
    .execute(&pool)
    .await
    .expect("insert trigger_instance");

    let (id, kind_id, user_defined): (String, String, i64) = sqlx::query_as(
        "SELECT id, kind_id, user_defined FROM trigger_instances WHERE id = 'inst-001'",
    )
    .fetch_one(&pool)
    .await
    .expect("fetch trigger_instance");

    assert_eq!(id, "inst-001");
    assert_eq!(kind_id, "twitch.chat.command");
    assert_eq!(user_defined, 1);
}

#[tokio::test]
async fn partial_unique_index_rejects_duplicate_default_per_kind() {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");

    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined) \
         VALUES ('default-001', 'twitch.chat.command', 'Default', '{}', 1, 0)",
    )
    .execute(&pool)
    .await
    .expect("first default insert");

    let result = sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined) \
         VALUES ('default-002', 'twitch.chat.command', 'Default 2', '{}', 1, 0)",
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "duplicate default per kind_id must be rejected by partial unique index"
    );

    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined) \
         VALUES ('user-001', 'twitch.chat.command', 'User Override', '{}', 1, 1)",
    )
    .execute(&pool)
    .await
    .expect("user_defined=1 must be allowed alongside a default instance");
}

#[tokio::test]
async fn restrict_fk_blocks_delete_of_referenced_trigger_instance() {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");

    sqlx::query(
        "INSERT INTO actions (id, name, queue_id, sub_actions) \
         VALUES ('action-001', 'test action', '00000000000000000000000000', '[]')",
    )
    .execute(&pool)
    .await
    .expect("insert action");

    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined) \
         VALUES ('inst-001', 'twitch.chat.command', 'Test', '{}', 1, 0)",
    )
    .execute(&pool)
    .await
    .expect("insert trigger_instance");

    sqlx::query(
        "INSERT INTO action_trigger_instances (action_id, trigger_instance_id, position) \
         VALUES ('action-001', 'inst-001', 0)",
    )
    .execute(&pool)
    .await
    .expect("insert join row");

    let result = sqlx::query("DELETE FROM trigger_instances WHERE id = 'inst-001'")
        .execute(&pool)
        .await;

    assert!(
        result.is_err(),
        "ON DELETE RESTRICT must block deletion of a trigger_instance referenced by action_trigger_instances"
    );

    sqlx::query("DELETE FROM actions WHERE id = 'action-001'")
        .execute(&pool)
        .await
        .expect("delete action cascades join row");

    sqlx::query("DELETE FROM trigger_instances WHERE id = 'inst-001'")
        .execute(&pool)
        .await
        .expect("delete trigger_instance succeeds after cascade removes join row");
}
