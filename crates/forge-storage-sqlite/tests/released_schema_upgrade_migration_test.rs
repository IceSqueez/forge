#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;

use std::str::FromStr;

use forge_storage::{DataProvider, EXPECTED_SCHEMA_VERSION};
use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use forge_types::{ActionId, Variant};
use sqlx::SqlitePool;
use tempfile::TempDir;

const RELEASED_SCHEMA_VERSION: i64 = 54;
const REGISTRY_FORMAT: i64 = 2;
const CREATED_AT: i64 = 1_761_328_800;
const STAMP: &str = "2026-10-01T12:00:00Z";
const CUSTOM_QUEUE: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0Q1";
const ACTION: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0A1";
const TRIGGER: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0T1";
const SUB_ACTIONS: &str = r#"[{"kind_id":"core.globals.toggle","config":{"key":{"type":"string","value":"flag"}},"enabled":true,"continue_on_error":false,"condition":null,"label":null},{"kind_id":"obs.scenes.switch_current","config":{"scene":{"type":"string","value":"%obs.scenes.current%"}},"enabled":true,"continue_on_error":false,"condition":null,"label":null}]"#;

async fn released_database(url: &str) -> SqlitePool {
    let pool = connect(url).await.expect("a database");
    MIGRATIONS
        .run_to(RELEASED_SCHEMA_VERSION, &pool)
        .await
        .expect("the released schema");
    seed(&pool).await;
    pool
}

async fn seed(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO queues (id, name, blocking, description, concurrency, paused) VALUES (?, 'Custom', 1, 'mine', 3, 1)",
    )
    .bind(CUSTOM_QUEUE)
    .execute(pool)
    .await
    .expect("a queue");
    sqlx::query(
        "INSERT INTO actions (id, sub_actions, name, queue_id, format_version) VALUES (?, ?, 'Flip', ?, ?)",
    )
    .bind(ACTION)
    .bind(SUB_ACTIONS)
    .bind(CUSTOM_QUEUE)
    .bind(REGISTRY_FORMAT)
    .execute(pool)
    .await
    .expect("an action");
    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, user_defined) VALUES (?, 'obs.scenes.current_changed', 'Scene', '{\"text\":\"%obs.scenes.current%\"}', 1)",
    )
    .bind(TRIGGER)
    .execute(pool)
    .await
    .expect("a trigger instance");
    sqlx::query(
        "INSERT INTO action_trigger_instances (action_id, trigger_instance_id, position) VALUES (?, ?, 0)",
    )
    .bind(ACTION)
    .bind(TRIGGER)
    .execute(pool)
    .await
    .expect("a binding");
    sqlx::query("INSERT INTO settings (key, value) VALUES ('ui.note', 'kept')")
        .execute(pool)
        .await
        .expect("a setting");
    sqlx::query(
        "INSERT INTO globals (name, value, type_tag, persisted, reads, writes, created_at, last_modified) VALUES ('counter', '{\"type\":\"int\",\"value\":7}', 'int', 1, 3, 4, ?, ?)",
    )
    .bind(CREATED_AT)
    .bind(CREATED_AT)
    .execute(pool)
    .await
    .expect("a global");
    sqlx::query(
        "INSERT INTO user_globals (broadcaster_id, user_id, name, value, type_tag, last_modified) VALUES ('b', 'u', 'points', '{\"type\":\"int\",\"value\":9}', 'int', ?)",
    )
    .bind(CREATED_AT)
    .execute(pool)
    .await
    .expect("a user global");
    sqlx::query(
        "INSERT INTO voice_aliases (id, viewer_id, viewer_name, engine_id, voice_id, state, created_at, updated_at) VALUES ('alias-1', 'twitch:42', 'ann', 'piper', 'amy', 'Active', ?, ?)",
    )
    .bind(STAMP)
    .bind(STAMP)
    .execute(pool)
    .await
    .expect("a voice alias");
    sqlx::query(
        "INSERT INTO soundboard_clips (id, name, file_path, output_device, created_at, builtin_id) VALUES ('clip-1', 'Airhorn', 'clip.wav', 'default', ?, 'airhorn')",
    )
    .bind(CREATED_AT)
    .execute(pool)
    .await
    .expect("a clip");
    sqlx::query("UPDATE tts_pipeline_settings SET max_length = 120 WHERE id = 1")
        .execute(pool)
        .await
        .expect("a pipeline limit");
}

async fn scalar_text(pool: &SqlitePool, statement: &'static str) -> String {
    sqlx::query_scalar(statement)
        .fetch_one(pool)
        .await
        .expect("a stored value")
}

#[tokio::test]
async fn every_migration_after_the_released_schema_applies_and_is_journaled() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = released_database(&common::database_url(&dir)).await;

    apply_migrations(&pool).await.expect("migrate");

    let journal: (i64, i64) = sqlx::query_as(
        "SELECT MAX(version), COUNT(*) FILTER (WHERE success = 0) FROM _sqlx_migrations",
    )
    .fetch_one(&pool)
    .await
    .expect("the journal");
    assert_eq!(journal, (i64::from(EXPECTED_SCHEMA_VERSION), 0));
}

#[tokio::test]
async fn upgrading_keeps_every_user_row_and_rewrites_what_changed_shape() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = released_database(&common::database_url(&dir)).await;

    apply_migrations(&pool).await.expect("migrate");

    let steps = scalar_text(
        &pool,
        "SELECT sub_actions FROM actions WHERE id = '01JBZ8Q4T5N7W3K2M9X6V1R0A1'",
    )
    .await;
    let overrides = scalar_text(
        &pool,
        "SELECT overrides FROM trigger_instances WHERE id = '01JBZ8Q4T5N7W3K2M9X6V1R0T1'",
    )
    .await;
    let trigger_kind = scalar_text(
        &pool,
        "SELECT kind_id FROM trigger_instances WHERE id = '01JBZ8Q4T5N7W3K2M9X6V1R0T1'",
    )
    .await;
    let alias_key = scalar_text(
        &pool,
        "SELECT alias_key FROM voice_aliases WHERE id = 'alias-1'",
    )
    .await;
    let clip_name = scalar_text(
        &pool,
        "SELECT name FROM soundboard_clips WHERE id = 'clip-1'",
    )
    .await;
    let setting = scalar_text(&pool, "SELECT value FROM settings WHERE key = 'ui.note'").await;
    let global = scalar_text(&pool, "SELECT value FROM globals WHERE name = 'counter'").await;
    let user_global = scalar_text(
        &pool,
        "SELECT value FROM user_globals WHERE user_id = 'u' AND name = 'points'",
    )
    .await;
    let queue: (String, i64) =
        sqlx::query_as("SELECT description, concurrency FROM queues WHERE id = ?")
            .bind(CUSTOM_QUEUE)
            .fetch_one(&pool)
            .await
            .expect("the queue");
    let pipeline: (i64, i64) = sqlx::query_as(
        "SELECT skip_longer_than, longer_than_max_chars FROM tts_pipeline_settings WHERE id = 1",
    )
    .fetch_one(&pool)
    .await
    .expect("the pipeline settings");
    let bindings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM action_trigger_instances")
        .fetch_one(&pool)
        .await
        .expect("the bindings");

    assert_eq!(
        (
            steps,
            overrides,
            trigger_kind,
            alias_key,
            clip_name,
            setting,
            global,
            user_global,
            queue,
            pipeline,
            bindings
        ),
        (
            SUB_ACTIONS
                .replace("\"key\"", "\"name\"")
                .replace("obs.scenes.switch_current", "obs.scene.switch_current")
                .replace("%obs.scenes.current%", "%obs.scene%"),
            "{\"text\":\"%obs.scene%\"}".to_owned(),
            "obs.scene.changed".to_owned(),
            "twitch:42".to_owned(),
            "MLG Airhorn".to_owned(),
            "kept".to_owned(),
            "{\"type\":\"int\",\"value\":7}".to_owned(),
            "{\"type\":\"int\",\"value\":9}".to_owned(),
            ("mine".to_owned(), 3),
            (1, 120),
            1
        )
    );
}

#[tokio::test]
async fn the_released_database_file_opens_through_the_backend_and_runs_its_migrated_action() {
    let dir = TempDir::new().expect("a temporary directory");
    let url = common::database_url(&dir);
    released_database(&url).await.close().await;

    let backend = common::sandboxed_backend_at(&url, common::TEST_KEY).await;

    let action = backend
        .action_repo()
        .get(ActionId::from_str(ACTION).expect("an action id"))
        .await
        .expect("the action")
        .expect("the seeded action");
    let toggle = &action.sub_actions[0];
    assert_eq!(
        (
            action.sub_actions.len(),
            toggle.config.get("name").cloned(),
            action.sub_actions[1].kind_id.clone()
        ),
        (
            2,
            Some(Variant::String("flag".to_owned())),
            "obs.scene.switch_current".to_owned()
        )
    );
}
