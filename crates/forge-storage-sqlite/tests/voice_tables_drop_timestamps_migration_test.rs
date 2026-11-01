#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;

use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_DROP: i64 = 73;
const STAMP: &str = "2026-10-01T12:00:00Z";

async fn seeded(dir: &TempDir) -> SqlitePool {
    let pool = connect(&common::database_url(dir))
        .await
        .expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_DROP, &pool)
        .await
        .expect("the schema before the drop");
    sqlx::query(
        "INSERT INTO voice_aliases (id, alias_key, viewer_name, engine_id, voice_id, pitch_semitones, rate_multiplier, state, created_at, updated_at)
         VALUES ('alias-1', 'twitch:42', 'ann', 'piper', 'amy', 1.5, 0.75, 'Active', ?, ?)",
    )
    .bind(STAMP)
    .bind(STAMP)
    .execute(&pool)
    .await
    .expect("a seeded alias");
    sqlx::query(
        "UPDATE ignore_profile
         SET excluded_voice_ids = '[\"v1\"]', excluded_locales = '[\"de\"]', updated_at = ?
         WHERE id = 1",
    )
    .bind(STAMP)
    .execute(&pool)
    .await
    .expect("a seeded ignore profile");
    pool
}

async fn columns(pool: &SqlitePool, table: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
        .bind(table)
        .fetch_all(pool)
        .await
        .expect("the columns")
}

#[tokio::test]
async fn migration_drops_the_timestamp_columns_and_keeps_every_other_alias_field() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = seeded(&dir).await;

    apply_migrations(&pool).await.expect("migrate");

    let alias: (String, String, String, String, f64, f64, String) = sqlx::query_as(
        "SELECT alias_key, viewer_name, engine_id, voice_id, pitch_semitones, rate_multiplier, state FROM voice_aliases WHERE id = 'alias-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("the alias");
    let stamps_left = columns(&pool, "voice_aliases")
        .await
        .iter()
        .any(|column| column == "created_at" || column == "updated_at");
    assert_eq!(
        (stamps_left, alias),
        (
            false,
            (
                "twitch:42".to_owned(),
                "ann".to_owned(),
                "piper".to_owned(),
                "amy".to_owned(),
                1.5,
                0.75,
                "Active".to_owned()
            )
        )
    );
}

#[tokio::test]
async fn migration_drops_the_ignore_profile_timestamp_and_keeps_its_lists() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = seeded(&dir).await;

    apply_migrations(&pool).await.expect("migrate");

    let lists: (String, String) = sqlx::query_as(
        "SELECT excluded_voice_ids, excluded_locales FROM ignore_profile WHERE id = 1",
    )
    .fetch_one(&pool)
    .await
    .expect("the ignore profile");
    assert_eq!(
        (columns(&pool, "ignore_profile").await, lists),
        (
            vec![
                "id".to_owned(),
                "excluded_voice_ids".to_owned(),
                "excluded_locales".to_owned()
            ],
            ("[\"v1\"]".to_owned(), "[\"de\"]".to_owned())
        )
    );
}
