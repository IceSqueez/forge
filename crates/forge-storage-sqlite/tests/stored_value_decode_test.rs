#![allow(clippy::expect_used)]
use forge_storage::{ActionRepo, StorageError, VoiceAliasRepo};
use forge_storage_sqlite::{SqliteActionRepo, SqliteVoiceAliasRepo, apply_migrations};
use sqlx::SqlitePool;

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");
    pool
}

#[tokio::test]
async fn an_action_with_an_unknown_execution_mode_fails_to_decode() {
    let pool = migrated_pool().await;
    let queue_id: String = sqlx::query_scalar("SELECT id FROM queues LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("a seeded queue");
    sqlx::query(
        "INSERT INTO actions (id, name, queue_id, execution_mode)
         VALUES ('01ARZ3NDEKTSV4RRFFQ69G5FAV', 'odd', ?, 'shuffle')",
    )
    .bind(queue_id)
    .execute(&pool)
    .await
    .expect("insert action row");

    let err = SqliteActionRepo::new(pool)
        .list()
        .await
        .expect_err("unknown execution mode must not decode to a default");

    assert!(
        err.to_string().contains("execution_mode"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn a_voice_alias_with_an_unknown_state_fails_to_decode() {
    let pool = migrated_pool().await;
    sqlx::query(
        "INSERT INTO voice_aliases
            (id, viewer_id, viewer_name, engine_id, voice_id, state, created_at, updated_at)
         VALUES ('a1', 'v1', 'ann', 'piper', 'voice', 'Muted', '0', '0')",
    )
    .execute(&pool)
    .await
    .expect("insert alias row");

    let err = SqliteVoiceAliasRepo::new(pool)
        .list()
        .await
        .expect_err("unknown alias state must not decode to Active");

    assert!(matches!(err, StorageError::Parse(_)), "got {err:?}");
}
