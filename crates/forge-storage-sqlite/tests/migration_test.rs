#![allow(clippy::expect_used)]

use forge_storage_sqlite::apply_migrations;

#[tokio::test]
async fn all_core_tables_exist_after_migration() {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    apply_migrations(&pool).await.expect("migrations apply");

    let present: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(&pool)
            .await
            .expect("query sqlite_master");

    for table in [
        "globals",
        "user_globals",
        "settings",
        "action_history",
        "credentials",
        "queues",
        "actions",
        "scripts",
        "event_log",
        "soundboard_clips",
        "voice_aliases",
        "ignore_profile",
        "replacement_rules",
        "viewers",
        "action_executions",
        "trigger_instances",
        "action_trigger_instances",
        "tts_filter_rules",
        "tts_pipeline_settings",
        "overlays",
    ] {
        assert!(
            present.iter().any(|name| name == table),
            "missing table after migrations: {table}"
        );
    }
}
