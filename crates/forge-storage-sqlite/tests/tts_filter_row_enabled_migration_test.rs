#![allow(clippy::expect_used)]
mod common;
use forge_storage::{SkipRegex, TtsFiltersRepo};
use forge_storage_sqlite::{MIGRATIONS, SqliteTtsFiltersRepo, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_ROW_SWITCHES: i64 = 63;

async fn migrated_with(stored_regexes: &str) -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let url = common::database_url(&dir);
    let pool = connect(&url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_ROW_SWITCHES, &pool)
        .await
        .expect("the schema before the switches");
    let seeded = sqlx::query("UPDATE tts_pipeline_settings SET skip_custom_regexes = ?")
        .bind(stored_regexes)
        .execute(&pool)
        .await
        .expect("the settings row stores the list");
    assert_eq!(seeded.rows_affected(), 1);
    apply_migrations(&pool).await.expect("the migrations apply");
    (pool, dir)
}

async fn stored_regexes(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT skip_custom_regexes FROM tts_pipeline_settings")
        .fetch_one(pool)
        .await
        .expect("the list reads")
}

#[tokio::test]
async fn stored_patterns_become_enabled_objects_in_their_stored_order() {
    let patterns: Vec<String> = (0..12).rev().map(|n| format!("^{n}$")).collect();
    let stored = serde_json::to_string(&patterns).expect("a json list");
    let (pool, _dir) = migrated_with(&stored).await;

    let written: serde_json::Value =
        serde_json::from_str(&stored_regexes(&pool).await).expect("the list stays json");

    let expected: Vec<serde_json::Value> = patterns
        .iter()
        .map(|pattern| serde_json::json!({ "pattern": pattern, "enabled": true }))
        .collect();
    assert_eq!(written, serde_json::Value::Array(expected));
}

#[tokio::test]
async fn a_pattern_needing_json_escapes_reads_back_unchanged_through_the_repo() {
    let pattern = r#"\d+"quoted"\\日本語"#;
    let stored = serde_json::to_string(&[pattern]).expect("a json list");
    let (pool, _dir) = migrated_with(&stored).await;

    let settings = SqliteTtsFiltersRepo::new(pool)
        .get_pipeline_settings()
        .await
        .expect("the migrated row decodes");

    assert_eq!(settings.skip_custom_regexes, [SkipRegex::enabled(pattern)]);
}

#[tokio::test]
async fn an_empty_list_stays_empty() {
    let (pool, _dir) = migrated_with("[]").await;

    assert_eq!(stored_regexes(&pool).await, "[]");
}

#[tokio::test]
async fn a_row_stored_before_the_switches_keeps_every_row_on() {
    let (pool, _dir) = migrated_with("[]").await;

    let switches: (i64, i64) = sqlx::query_as(
        "SELECT skip_prefix_enabled, output_max_duration_enabled FROM tts_pipeline_settings",
    )
    .fetch_one(&pool)
    .await
    .expect("the new columns read");

    assert_eq!(switches, (1, 1));
}
