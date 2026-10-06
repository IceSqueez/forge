#![allow(clippy::expect_used)]

use std::collections::BTreeMap;

use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_AUTHOR_IDENTITY: i64 = 50;

struct SeededDb {
    pool: SqlitePool,
    _dir: TempDir,
}

async fn db_at_previous_schema() -> SeededDb {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = format!("sqlite://{}", dir.path().join("forge.db").display());
    let pool = connect(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_AUTHOR_IDENTITY, &pool)
        .await
        .expect("the schema before author identity");
    SeededDb { pool, _dir: dir }
}

async fn execute(pool: &SqlitePool, sql: &'static str) {
    sqlx::query(sql)
        .execute(pool)
        .await
        .expect("fixture statement");
}

async fn insert_viewer(pool: &SqlitePool, platform: &str, viewer_id: &str, username: &str) {
    sqlx::query(
        "INSERT INTO viewers (platform, viewer_id, username, first_seen_at, last_seen_at)
         VALUES (?, ?, ?, 0, 0)",
    )
    .bind(platform)
    .bind(viewer_id)
    .bind(username)
    .execute(pool)
    .await
    .expect("a viewer row");
}

async fn insert_chat(pool: &SqlitePool, id: &str, source: &str, author: &str) {
    sqlx::query(
        "INSERT INTO chat_history (id, event_id, source, received_at, author)
         VALUES (?, 'event', ?, 0, ?)",
    )
    .bind(id)
    .bind(source)
    .bind(author)
    .execute(pool)
    .await
    .expect("a chat row");
}

async fn insert_alias(pool: &SqlitePool, id: &str, viewer_id: &str, updated_at: &str) {
    sqlx::query(
        "INSERT INTO voice_aliases
            (id, viewer_id, viewer_name, engine_id, voice_id, created_at, updated_at)
         VALUES (?, ?, 'name', 'piper', 'voice', '2026-01-01 00:00:00', ?)",
    )
    .bind(id)
    .bind(viewer_id)
    .bind(updated_at)
    .execute(pool)
    .await
    .expect("an alias row");
}

async fn author_ids(pool: &SqlitePool) -> BTreeMap<String, Option<String>> {
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, author_id FROM chat_history")
            .fetch_all(pool)
            .await
            .expect("chat rows");
    rows.into_iter().collect()
}

#[tokio::test]
async fn backfill_fills_author_id_only_for_a_single_case_insensitive_viewer_match() {
    let db = db_at_previous_schema().await;
    insert_viewer(&db.pool, "twitch", "111", "alice").await;
    insert_viewer(&db.pool, "twitch", "222", "bob").await;
    insert_viewer(&db.pool, "twitch", "333", "BOB").await;
    insert_viewer(&db.pool, "kick", "444", "carol").await;
    insert_chat(&db.pool, "unique", "twitch", "Alice").await;
    insert_chat(&db.pool, "ambiguous", "twitch", "bob").await;
    insert_chat(&db.pool, "unmatched", "twitch", "nobody").await;
    insert_chat(&db.pool, "other_platform", "youtube", "carol").await;

    apply_migrations(&db.pool).await.expect("migrate");

    let expected: BTreeMap<String, Option<String>> = [
        ("unique", Some("111")),
        ("ambiguous", None),
        ("unmatched", None),
        ("other_platform", None),
    ]
    .into_iter()
    .map(|(id, author_id)| (id.to_owned(), author_id.map(str::to_owned)))
    .collect();
    assert_eq!(author_ids(&db.pool).await, expected);
}

#[tokio::test]
async fn dedupe_keeps_only_the_newest_alias_per_viewer_with_rowid_breaking_ties() {
    let db = db_at_previous_schema().await;
    insert_alias(&db.pool, "v1-old", "v1", "2026-01-01 00:00:00").await;
    insert_alias(&db.pool, "v1-newest", "v1", "2026-03-01 00:00:00").await;
    insert_alias(&db.pool, "v1-middle", "v1", "2026-02-01 00:00:00").await;
    insert_alias(&db.pool, "v2-first", "v2", "2026-05-01 00:00:00").await;
    insert_alias(&db.pool, "v2-later-row", "v2", "2026-05-01 00:00:00").await;
    insert_alias(&db.pool, "v3-only", "v3", "2026-01-01 00:00:00").await;

    apply_migrations(&db.pool).await.expect("migrate");

    let survivors: Vec<String> = sqlx::query_scalar("SELECT id FROM voice_aliases ORDER BY id")
        .fetch_all(&db.pool)
        .await
        .expect("alias ids");
    assert_eq!(survivors, ["v1-newest", "v2-later-row", "v3-only"]);
}

#[tokio::test]
async fn a_second_alias_row_for_the_same_viewer_is_rejected_after_migration() {
    let db = db_at_previous_schema().await;
    apply_migrations(&db.pool).await.expect("migrate");
    insert_alias(&db.pool, "first", "v1", "2026-01-01 00:00:00").await;

    let duplicate = sqlx::query(
        "INSERT INTO voice_aliases
            (id, viewer_id, viewer_name, engine_id, voice_id, created_at, updated_at)
         VALUES ('second', 'v1', 'name', 'piper', 'voice', 'x', 'x')",
    )
    .execute(&db.pool)
    .await;

    assert!(duplicate.is_err());
}

#[tokio::test]
async fn the_retired_store_limit_setting_is_removed_and_other_settings_survive() {
    let db = db_at_previous_schema().await;
    execute(
        &db.pool,
        "INSERT INTO settings (key, value) VALUES
            ('chat_history.store_limit', '5000'),
            ('chat_history.display_limit', '300')",
    )
    .await;

    apply_migrations(&db.pool).await.expect("migrate");

    let keys: Vec<String> =
        sqlx::query_scalar("SELECT key FROM settings WHERE key LIKE 'chat_history.%' ORDER BY key")
            .fetch_all(&db.pool)
            .await
            .expect("setting keys");
    assert_eq!(keys, ["chat_history.display_limit"]);
}
