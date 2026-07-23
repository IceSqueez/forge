#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage_sqlite::{apply_migrations, connect};
use sqlx::SqlitePool;

const MIGRATION_SQL: &str = include_str!("../migrations/0038_twitch_kind_id_rename.sql");

const REMOVED_SLOT_ID: &str = "twitch.guest_star.slot_updated";
const SURVIVING_GUEST_ID: &str = "twitch.guest_star.guest_updated";

const STABLE_DESCRIPTOR_IDS: &[&str] = &[
    "twitch.chat.message",
    "twitch.channel.ban",
    "twitch.support.subscriber",
    "twitch.channel_points.redemption",
    "twitch.channel_points.redemption_updated",
    "twitch.automod.message_updated",
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
        .expect("run 0038 migration sql");
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
async fn removed_guest_star_slot_descriptor_consolidates_into_guest_updated() {
    let pool = fresh_pool().await;
    seed(&pool, "def-slot", REMOVED_SLOT_ID, 0).await;
    seed(&pool, "user-slot", REMOVED_SLOT_ID, 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-slot").await,
        None,
        "the orphaned default slot descriptor row must be deleted; its descriptor no longer exists"
    );
    assert_eq!(
        kind_id_of(&pool, "user-slot").await.as_deref(),
        Some(SURVIVING_GUEST_ID),
        "user rows for the removed slot descriptor must fold into the surviving guest descriptor id"
    );
}

#[tokio::test]
async fn stable_twitch_descriptor_ids_are_left_untouched() {
    let pool = fresh_pool().await;
    for (i, id) in STABLE_DESCRIPTOR_IDS.iter().enumerate() {
        seed(&pool, &format!("def-{i}"), id, 0).await;
        seed(&pool, &format!("user-{i}"), id, 1).await;
    }

    run_migration(&pool).await;

    for (i, id) in STABLE_DESCRIPTOR_IDS.iter().enumerate() {
        assert_eq!(
            kind_id_of(&pool, &format!("def-{i}")).await.as_deref(),
            Some(*id),
            "descriptor id {id} was not renamed this campaign and must survive migration verbatim"
        );
        assert_eq!(
            kind_id_of(&pool, &format!("user-{i}")).await.as_deref(),
            Some(*id),
        );
    }
}

#[tokio::test]
async fn core_kind_ids_are_left_untouched() {
    let pool = fresh_pool().await;
    seed(&pool, "core-log", "core.log.write", 0).await;
    seed(&pool, "script-event", "script.event.custom", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "core-log").await.as_deref(),
        Some("core.log.write"),
    );
    assert_eq!(
        kind_id_of(&pool, "script-event").await.as_deref(),
        Some("script.event.custom"),
    );
}

#[tokio::test]
async fn migration_is_idempotent_on_already_consolidated_rows() {
    let pool = fresh_pool().await;
    seed(&pool, "def-guest", SURVIVING_GUEST_ID, 0).await;
    seed(&pool, "user-guest", SURVIVING_GUEST_ID, 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-guest").await.as_deref(),
        Some(SURVIVING_GUEST_ID),
    );
    assert_eq!(
        kind_id_of(&pool, "user-guest").await.as_deref(),
        Some(SURVIVING_GUEST_ID),
    );
}
