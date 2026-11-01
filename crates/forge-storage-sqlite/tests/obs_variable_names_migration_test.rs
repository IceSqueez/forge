#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;

use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_RENAME: i64 = 71;
const REGISTRY_FORMAT: i64 = 2;
const ACTION_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0A1";
const TRIGGER_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0T1";

const STORED: &str = "scenes %obs.scene_names% %obs.scenes.all_names% now %obs.scenes.current% sources %obs.sources.all_names% collections %obs.collection.all_names% profiles %obs.profile.all_names% input %obs.input.settings% %obs.input.kind% keep %obs.scenes.names% %obs.other% obs.scene_names";
const MIGRATED: &str = "scenes %obs.scenes.names% %obs.scenes.names% now %obs.scene% sources %obs.sources.names% collections %obs.collections.names% profiles %obs.profiles.names% input %obs.source.settings% %obs.source.kind% keep %obs.scenes.names% %obs.other% obs.scene_names";
const UNRELATED: &str = "plain %user% text with no obs variables";

async fn seeded(dir: &TempDir, text: &str) -> SqlitePool {
    let pool = connect(&common::database_url(dir))
        .await
        .expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_RENAME, &pool)
        .await
        .expect("the schema before the rename");
    sqlx::query(
        "INSERT INTO actions (id, sub_actions, name, queue_id, format_version) VALUES (?, ?, 'action', ?, ?)",
    )
    .bind(ACTION_ID)
    .bind(text)
    .bind(common::DEFAULT_QUEUE)
    .bind(REGISTRY_FORMAT)
    .execute(&pool)
    .await
    .expect("a seeded action");
    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, user_defined) VALUES (?, 'twitch.chat.message', 'trigger', ?, 1)",
    )
    .bind(TRIGGER_ID)
    .bind(text)
    .execute(&pool)
    .await
    .expect("a seeded trigger instance");
    pool
}

async fn stored_pair(pool: &SqlitePool) -> (String, String) {
    let action = sqlx::query_scalar("SELECT sub_actions FROM actions WHERE id = ?")
        .bind(ACTION_ID)
        .fetch_one(pool)
        .await
        .expect("the action");
    let trigger = sqlx::query_scalar("SELECT overrides FROM trigger_instances WHERE id = ?")
        .bind(TRIGGER_ID)
        .fetch_one(pool)
        .await
        .expect("the trigger instance");
    (action, trigger)
}

#[tokio::test]
async fn migration_rewrites_only_retired_variable_names() {
    for (label, input, expected) in [
        (
            "migration_rewrites_every_retired_variable_name_in_actions_and_trigger_overrides",
            STORED,
            (MIGRATED.to_owned(), MIGRATED.to_owned()),
        ),
        (
            "migration_leaves_text_without_obs_variables_untouched",
            UNRELATED,
            (UNRELATED.to_owned(), UNRELATED.to_owned()),
        ),
    ] {
        let dir = TempDir::new().expect("a temporary directory");
        let pool = seeded(&dir, input).await;

        apply_migrations(&pool).await.expect("migrate");

        assert_eq!(stored_pair(&pool).await, expected, "{label}");
    }
}

#[tokio::test]
async fn a_second_migration_run_leaves_the_rewritten_text_as_it_was() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = seeded(&dir, STORED).await;
    apply_migrations(&pool).await.expect("first migrate");

    apply_migrations(&pool).await.expect("second migrate");

    assert_eq!(
        stored_pair(&pool).await,
        (MIGRATED.to_owned(), MIGRATED.to_owned())
    );
}
