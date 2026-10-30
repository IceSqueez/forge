#![allow(clippy::expect_used)]
use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_ROLES: i64 = 62;
const ALERT_QUEUE: &str = "Overlay Alerts";
const ALERT_ROLE: &str = "overlay_alerts";
const FIRST_ID: &str = "01JAAAAAAAAAAAAAAAAAAAAAAA";
const SECOND_ID: &str = "01JBBBBBBBBBBBBBBBBBBBBBBB";

async fn migrated_with(queues: &[(&str, &str)]) -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_ROLES, &pool)
        .await
        .expect("the schema before the roles");
    for (id, name) in queues {
        sqlx::query(
            "INSERT INTO queues (id, name, description, blocking, concurrency)
             VALUES (?, ?, '', 1, 1)",
        )
        .bind(id)
        .bind(name)
        .execute(&pool)
        .await
        .expect("a stored queue");
    }
    apply_migrations(&pool).await.expect("the migrations apply");
    (pool, dir)
}

async fn role_holders(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM queues WHERE role IS NOT NULL ORDER BY id")
        .fetch_all(pool)
        .await
        .expect("the role column reads")
}

#[tokio::test]
async fn the_queue_with_the_alert_name_receives_the_role() {
    let (pool, _dir) = migrated_with(&[(FIRST_ID, "Chat"), (SECOND_ID, ALERT_QUEUE)]).await;

    assert_eq!(role_holders(&pool).await, [SECOND_ID]);
}

#[tokio::test]
async fn of_two_queues_with_the_alert_name_only_the_first_receives_the_role() {
    let (pool, _dir) = migrated_with(&[(SECOND_ID, ALERT_QUEUE), (FIRST_ID, ALERT_QUEUE)]).await;

    assert_eq!(role_holders(&pool).await, [FIRST_ID]);
}

#[tokio::test]
async fn a_database_without_the_alert_queue_gives_the_role_to_nobody() {
    let (pool, _dir) = migrated_with(&[(FIRST_ID, "Chat")]).await;

    assert!(role_holders(&pool).await.is_empty());
}

#[tokio::test]
async fn a_second_holder_of_a_role_is_refused_while_any_number_of_queues_hold_none() {
    let (pool, _dir) = migrated_with(&[(FIRST_ID, ALERT_QUEUE), (SECOND_ID, "Chat")]).await;

    let refused = sqlx::query("UPDATE queues SET role = ? WHERE id = ?")
        .bind(ALERT_ROLE)
        .bind(SECOND_ID)
        .execute(&pool)
        .await;

    let roleless: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM queues WHERE role IS NULL")
        .fetch_one(&pool)
        .await
        .expect("the count reads");
    assert!(refused.is_err(), "{refused:?}");
    assert!(roleless >= 2, "{roleless}");
}
