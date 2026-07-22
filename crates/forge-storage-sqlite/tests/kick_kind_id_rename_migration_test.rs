#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage_sqlite::{apply_migrations, connect};
use sqlx::SqlitePool;

const MIGRATION_SQL: &str = include_str!("../migrations/0019_kick_kind_id_rename.sql");

async fn fresh_pool() -> SqlitePool {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");
    pool
}

async fn run_migration(pool: &SqlitePool) {
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(pool)
        .await
        .expect("run 0019 migration sql");
}

async fn seed(pool: &SqlitePool, id: &str, kind_id: &str, user_defined: i64) {
    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined) \
         VALUES (?, ?, 'seed', '{}', 1, ?)",
    )
    .bind(id)
    .bind(kind_id)
    .bind(user_defined)
    .execute(pool)
    .await
    .expect("seed trigger_instance");
}

async fn kind_id_of(pool: &SqlitePool, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT kind_id FROM trigger_instances WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .expect("fetch kind_id")
}

async fn link_action_to_trigger(pool: &SqlitePool, action_id: &str, trigger_instance_id: &str) {
    sqlx::query("INSERT INTO actions (id, name, queue_id) VALUES (?, 'seed-action', ?)")
        .bind(action_id)
        .bind("00000000000000000000000000")
        .execute(pool)
        .await
        .expect("seed action");
    sqlx::query(
        "INSERT INTO action_trigger_instances (action_id, trigger_instance_id, position) \
         VALUES (?, ?, 0)",
    )
    .bind(action_id)
    .bind(trigger_instance_id)
    .execute(pool)
    .await
    .expect("link action to trigger_instance");
}

async fn link_count_for(pool: &SqlitePool, trigger_instance_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM action_trigger_instances WHERE trigger_instance_id = ?",
    )
    .bind(trigger_instance_id)
    .fetch_one(pool)
    .await
    .expect("count links")
}

#[tokio::test]
async fn default_row_with_old_kick_id_is_renamed_in_place() {
    let pool = fresh_pool().await;
    seed(&pool, "def-chat", "kick.chat", 0).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-chat").await.as_deref(),
        Some("kick.chat.message"),
        "default (user_defined=0) row must be renamed in place - deleting it would abort the migration via the action_trigger_instances ON DELETE RESTRICT FK"
    );
}

#[tokio::test]
async fn fk_linked_default_row_survives_migration_and_keeps_its_action_link() {
    let pool = fresh_pool().await;
    seed(&pool, "def-sub-gift", "kick.sub_gift", 0).await;
    link_action_to_trigger(&pool, "act-sub-gift", "def-sub-gift").await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-sub-gift").await.as_deref(),
        Some("kick.channel.subscription_gift"),
        "the FK-referenced default row must be renamed in place, not deleted",
    );
    assert_eq!(
        link_count_for(&pool, "def-sub-gift").await,
        1,
        "the action_trigger_instances link must still point at the same row id",
    );
}

#[tokio::test]
async fn user_defined_rows_are_renamed_to_canonical_ids() {
    let renames = [
        ("kick.chat", "kick.chat.message"),
        ("kick.message_deleted", "kick.chat.message_deleted"),
        ("kick.ban", "kick.channel.banned"),
        ("kick.sub", "kick.channel.subscriber"),
        ("kick.sub_gift", "kick.channel.subscription_gift"),
        ("kick.host", "kick.channel.host_received"),
    ];

    let pool = fresh_pool().await;
    for (i, (old, _new)) in renames.iter().enumerate() {
        seed(&pool, &format!("user-{i}"), old, 1).await;
    }

    run_migration(&pool).await;

    for (i, (old, new)) in renames.iter().enumerate() {
        assert_eq!(
            kind_id_of(&pool, &format!("user-{i}")).await.as_deref(),
            Some(*new),
            "user_defined row {old} must be rewritten to {new}"
        );
    }
}

#[tokio::test]
async fn unmapped_kind_ids_are_left_untouched() {
    let pool = fresh_pool().await;
    seed(&pool, "def-unmapped", "kick.chat.message", 0).await;
    seed(&pool, "user-unmapped", "twitch.chat.message", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-unmapped").await.as_deref(),
        Some("kick.chat.message"),
        "unmapped default row must survive untouched",
    );
    assert_eq!(
        kind_id_of(&pool, "user-unmapped").await.as_deref(),
        Some("twitch.chat.message"),
        "unmapped user row must survive untouched",
    );
}

#[tokio::test]
async fn migration_is_noop_when_no_legacy_ids_present() {
    let pool = fresh_pool().await;
    seed(&pool, "def-canonical", "kick.chat.message", 0).await;
    seed(&pool, "user-canonical", "kick.channel.banned", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-canonical").await.as_deref(),
        Some("kick.chat.message"),
    );
    assert_eq!(
        kind_id_of(&pool, "user-canonical").await.as_deref(),
        Some("kick.channel.banned"),
    );
}
