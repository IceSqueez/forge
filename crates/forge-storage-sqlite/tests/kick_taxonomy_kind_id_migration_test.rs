#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage_sqlite::{apply_migrations, connect};
use sqlx::SqlitePool;

const MIGRATION_SQL: &str = include_str!("../migrations/0037_kick_kind_id_rename.sql");

const RENAMES: &[(&str, &str)] = &[
    ("kick.chat.message", "kick.chat.message.sent"),
    ("kick.chat.message_deleted", "kick.chat.message.deleted"),
    ("kick.channel.banned", "kick.moderation.banned"),
    ("kick.channel.subscriber", "kick.channel.subscribed"),
    (
        "kick.channel.subscription_gift",
        "kick.channel.subscription.gifts",
    ),
    ("kick.channel.host_received", "kick.channel.hosted"),
    (
        "kick.channel.livestream_status",
        "kick.livestream.status.updated",
    ),
    (
        "kick.channel.livestream_metadata",
        "kick.livestream.metadata.updated",
    ),
    (
        "kick.channel.reward_redeemed",
        "kick.channel.reward.redemption.updated",
    ),
];

async fn fresh_pool() -> SqlitePool {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");
    pool
}

async fn run_migration(pool: &SqlitePool) {
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(pool)
        .await
        .expect("run 0037 migration sql");
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

#[tokio::test]
async fn all_taxonomy_renames_apply_to_user_rows() {
    let pool = fresh_pool().await;
    for (i, (old, _new)) in RENAMES.iter().enumerate() {
        seed(&pool, &format!("user-{i}"), old, 1).await;
    }

    run_migration(&pool).await;

    for (i, (old, new)) in RENAMES.iter().enumerate() {
        assert_eq!(
            kind_id_of(&pool, &format!("user-{i}")).await.as_deref(),
            Some(*new),
            "user_defined row {old} must be rewritten to {new}"
        );
    }
}

#[tokio::test]
async fn default_rows_are_renamed_in_place_not_deleted() {
    let pool = fresh_pool().await;
    seed(&pool, "def-ban", "kick.channel.banned", 0).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-ban").await.as_deref(),
        Some("kick.moderation.banned"),
        "default (user_defined=0) row must be renamed in place, not deleted"
    );
}

#[tokio::test]
async fn chat_command_kind_id_is_left_untouched() {
    let pool = fresh_pool().await;
    seed(&pool, "def-command", "kick.chat.command", 0).await;
    seed(&pool, "user-command", "kick.chat.command", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-command").await.as_deref(),
        Some("kick.chat.command"),
    );
    assert_eq!(
        kind_id_of(&pool, "user-command").await.as_deref(),
        Some("kick.chat.command"),
    );
}

#[tokio::test]
async fn unmapped_kind_ids_are_left_untouched() {
    let pool = fresh_pool().await;
    seed(&pool, "user-unmapped", "twitch.chat.message", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "user-unmapped").await.as_deref(),
        Some("twitch.chat.message"),
    );
}

#[tokio::test]
async fn migration_is_noop_when_canonical_ids_present() {
    let pool = fresh_pool().await;
    seed(&pool, "def-canonical", "kick.moderation.banned", 0).await;
    seed(&pool, "user-canonical", "kick.channel.subscribed", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-canonical").await.as_deref(),
        Some("kick.moderation.banned"),
    );
    assert_eq!(
        kind_id_of(&pool, "user-canonical").await.as_deref(),
        Some("kick.channel.subscribed"),
    );
}
