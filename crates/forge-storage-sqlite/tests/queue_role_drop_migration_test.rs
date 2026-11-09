#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;
use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_DROP: i64 = 78;
const ALERT_QUEUE_ID: &str = "01JAAAAAAAAAAAAAAAAAAAAAAA";
const PLAIN_QUEUE_ID: &str = "01JBBBBBBBBBBBBBBBBBBBBBBB";

async fn upgraded_with_a_role_holder() -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let pool = connect(&common::database_url(&dir))
        .await
        .expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_DROP, &pool)
        .await
        .expect("the schema that still has queue roles");
    sqlx::query(
        "INSERT INTO queues (id, name, description, concurrency, role)
         VALUES (?, 'Stream alerts', 'shown one at a time', 1, 'overlay_alerts'),
                (?, 'Chat', '', 3, NULL)",
    )
    .bind(ALERT_QUEUE_ID)
    .bind(PLAIN_QUEUE_ID)
    .execute(&pool)
    .await
    .expect("the queues are stored");
    apply_migrations(&pool)
        .await
        .expect("the migrations apply over a role holder");
    (pool, dir)
}

#[tokio::test]
async fn dropping_queue_roles_keeps_every_queue_row_as_it_was() {
    let (pool, _dir) = upgraded_with_a_role_holder().await;

    let kept: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT id, name, description, concurrency FROM queues WHERE id IN (?, ?) ORDER BY id",
    )
    .bind(ALERT_QUEUE_ID)
    .bind(PLAIN_QUEUE_ID)
    .fetch_all(&pool)
    .await
    .expect("the queues read");

    assert_eq!(
        kept,
        [
            (
                ALERT_QUEUE_ID.to_owned(),
                "Stream alerts".to_owned(),
                "shown one at a time".to_owned(),
                1
            ),
            (
                PLAIN_QUEUE_ID.to_owned(),
                "Chat".to_owned(),
                String::new(),
                3
            ),
        ],
    );
}

#[tokio::test]
async fn dropping_queue_roles_removes_the_role_column() {
    let (pool, _dir) = upgraded_with_a_role_holder().await;

    let role_columns: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('queues') WHERE name = 'role'")
            .fetch_one(&pool)
            .await
            .expect("the table info reads");

    assert_eq!(role_columns, 0);
}
