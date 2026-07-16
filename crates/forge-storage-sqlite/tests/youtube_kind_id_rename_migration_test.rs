#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Regression coverage for the YouTube `kind_id` rewrite data migration.
//!
//! The migration is pure DML (DELETE/UPDATE on `trigger_instances` keyed by
//! `kind_id`), so it is safe to re-run against rows seeded after the schema is
//! in place. Each test applies the full schema, seeds legacy rows, then runs
//! the REAL migration SQL (loaded verbatim from the file, never a copy) so the
//! production statements - not a reimplementation - are under test.

use forge_storage_sqlite::{apply_migrations, connect};
use sqlx::SqlitePool;

const MIGRATION_SQL: &str = include_str!("../migrations/0018_youtube_kind_id_rename.sql");

async fn fresh_pool() -> SqlitePool {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");
    pool
}

async fn run_migration(pool: &SqlitePool) {
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(pool)
        .await
        .expect("run 0018 migration sql");
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
async fn default_row_with_old_youtube_id_is_deleted() {
    let pool = fresh_pool().await;
    seed(&pool, "def-super-chat", "youtube.support.super_chat", 0).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-super-chat").await,
        None,
        "default (user_defined=0) row for an old YouTube id must be deleted so boot re-seeds the canonical default"
    );
}

#[tokio::test]
async fn user_defined_rows_are_renamed_to_canonical_ids() {
    // The six straight 1-to-1 renames. ban/timeout consolidation is covered
    // separately because both collapse onto a single target id.
    let renames = [
        ("youtube.support.super_chat", "youtube.chat.super_chat"),
        (
            "youtube.support.super_sticker",
            "youtube.chat.super_sticker",
        ),
        ("youtube.support.new_member", "youtube.channel.member"),
        (
            "youtube.support.member_milestone",
            "youtube.channel.member_milestone",
        ),
        (
            "youtube.channel.live_broadcast_started",
            "youtube.stream.online",
        ),
        (
            "youtube.channel.live_broadcast_ended",
            "youtube.stream.offline",
        ),
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
async fn user_defined_ban_and_timeout_both_consolidate_to_user_banned() {
    let pool = fresh_pool().await;
    seed(&pool, "user-ban", "youtube.moderation.ban", 1).await;
    seed(&pool, "user-timeout", "youtube.moderation.timeout", 1).await;

    run_migration(&pool).await;

    // Both override rows survive (each is a distinct user-configured trigger)
    // and both now point at the consolidated canonical id.
    assert_eq!(
        kind_id_of(&pool, "user-ban").await.as_deref(),
        Some("youtube.channel.user_banned"),
    );
    assert_eq!(
        kind_id_of(&pool, "user-timeout").await.as_deref(),
        Some("youtube.channel.user_banned"),
    );

    let surviving: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM trigger_instances WHERE kind_id = 'youtube.channel.user_banned'",
    )
    .fetch_one(&pool)
    .await
    .expect("count consolidated rows");
    assert_eq!(
        surviving, 2,
        "both user overrides must be preserved post-consolidation, not merged"
    );
}

#[tokio::test]
async fn unmapped_kind_ids_are_left_untouched() {
    let pool = fresh_pool().await;
    // A default row and a user row, neither in the rewrite set.
    seed(&pool, "def-unmapped", "youtube.chat.message", 0).await;
    seed(&pool, "user-unmapped", "twitch.chat.message", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-unmapped").await.as_deref(),
        Some("youtube.chat.message"),
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
    seed(&pool, "def-canonical", "youtube.chat.super_chat", 0).await;
    seed(&pool, "user-canonical", "youtube.channel.user_banned", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-canonical").await.as_deref(),
        Some("youtube.chat.super_chat"),
    );
    assert_eq!(
        kind_id_of(&pool, "user-canonical").await.as_deref(),
        Some("youtube.channel.user_banned"),
    );
}
