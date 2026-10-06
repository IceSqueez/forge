#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::{ActionRepo, DataProvider};
use forge_storage_sqlite::{SqliteActionRepo, apply_migrations, connect, registry_migration};
use forge_types::{Action, ActionId, ExecutionMode, QueueId, SubActionStep, Variant};
use sqlx::SqlitePool;

mod common;

const TEST_KEY: [u8; 32] = [0xab; 32];

#[tokio::test]
async fn old_sub_action_spec_json_is_converted_to_sub_action_step() {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");

    let action_id = ActionId::new();

    sqlx::query(
        "INSERT INTO actions (id, name, queue_id, sub_actions) VALUES (?, 'test', '00000000000000000000000000', ?)",
    )
    .bind(action_id.to_string())
    .bind(r#"[{"SendChat":{"message":"Hello!","target":"twitch"}},{"Delay":{"ms":1000}}]"#)
    .execute(&pool)
    .await
    .expect("insert action");

    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("migrate");

    let (sub_actions_json, version): (String, i64) =
        sqlx::query_as("SELECT sub_actions, format_version FROM actions WHERE id = ?")
            .bind(action_id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(version, 2);

    let steps: Vec<SubActionStep> = serde_json::from_str(&sub_actions_json).expect("parse steps");
    assert_eq!(steps.len(), 2);

    assert_eq!(steps[0].kind_id, "twitch.chat.send_message");
    assert!(steps[0].enabled);
    assert_eq!(
        steps[0].config.get("message"),
        Some(&Variant::String("Hello!".to_owned()))
    );
    assert_eq!(
        steps[0].config.get("target"),
        Some(&Variant::String("twitch".to_owned()))
    );

    assert_eq!(steps[1].kind_id, "core.logic.wait");
    assert_eq!(steps[1].config.get("ms"), Some(&Variant::Int(1000)));
}

#[tokio::test]
async fn migration_is_idempotent_on_second_call() {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");

    let action_id = ActionId::new();

    sqlx::query(
        "INSERT INTO actions (id, name, queue_id, sub_actions) VALUES (?, 'test', '00000000000000000000000000', ?)",
    )
    .bind(action_id.to_string())
    .bind(r#"[{"SetGlobal":{"name":"counter","value":{"type":"int","value":0},"persisted":true}}]"#)
    .execute(&pool)
    .await
    .expect("insert action");

    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("first migrate");

    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("second migrate - must not error");

    let (sub_actions_json, version): (String, i64) =
        sqlx::query_as("SELECT sub_actions, format_version FROM actions WHERE id = ?")
            .bind(action_id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(version, 2);

    let steps: Vec<SubActionStep> = serde_json::from_str(&sub_actions_json).expect("parse steps");
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].kind_id, "core.globals.set");
    assert_eq!(
        steps[0].config.get("name"),
        Some(&Variant::String("counter".to_owned()))
    );
    assert_eq!(steps[0].config.get("persisted"), Some(&Variant::Bool(true)));
}

#[tokio::test]
async fn open_with_key_applies_action_registry_migration_on_boot() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let db_path = dir.path().join("test.db");
    let url = format!("sqlite:{}", db_path.display());

    let action_id = ActionId::new();

    {
        let pool = connect(&url).await.expect("connect");
        apply_migrations(&pool).await.expect("apply migrations");

        sqlx::query(
            "INSERT INTO actions (id, name, queue_id, sub_actions) VALUES (?, 'boot test', '00000000000000000000000000', ?)",
        )
        .bind(action_id.to_string())
        .bind(r#"[{"GetGlobal":{"name":"score","arg_name":"score_val"}}]"#)
        .execute(&pool)
        .await
        .expect("insert action");
    }

    let backend = common::sandboxed_backend(&url, TEST_KEY).await;

    let action = backend
        .action_repo()
        .get(action_id)
        .await
        .expect("get action")
        .expect("action present");

    assert_eq!(action.sub_actions.len(), 1);
    assert_eq!(action.sub_actions[0].kind_id, "core.globals.get");
    assert_eq!(
        action.sub_actions[0].config.get("name"),
        Some(&Variant::String("score".to_owned()))
    );
    assert_eq!(
        action.sub_actions[0].config.get("arg_name"),
        Some(&Variant::String("score_val".to_owned()))
    );
}

const DEFAULT_QUEUE: &str = "00000000000000000000000000";
const KIND_THE_DISCORD_CONVERTER_REWRITES: &str = "discord.post_text";

async fn migrated_pool() -> (SqlitePool, SqliteActionRepo) {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");
    let repo = SqliteActionRepo::new(pool.clone());
    (pool, repo)
}

fn action_with_step(kind_id: &str) -> Action {
    Action {
        id: ActionId::new(),
        name: "fresh".to_owned(),
        group: None,
        queue_id: DEFAULT_QUEUE.parse::<QueueId>().expect("queue id"),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![SubActionStep {
            kind_id: kind_id.to_owned(),
            config: Default::default(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }],
    }
}

async fn insert_legacy_row(pool: &SqlitePool, id: ActionId, sub_actions_json: &str) {
    sqlx::query("INSERT INTO actions (id, name, queue_id, sub_actions) VALUES (?, 'legacy', ?, ?)")
        .bind(id.to_string())
        .bind(DEFAULT_QUEUE)
        .bind(sub_actions_json)
        .execute(pool)
        .await
        .expect("insert legacy action row");
}

async fn first_kind_id(repo: &SqliteActionRepo, id: ActionId) -> String {
    repo.get(id)
        .await
        .expect("get action")
        .expect("action present")
        .sub_actions[0]
        .kind_id
        .clone()
}

#[tokio::test]
async fn an_action_written_through_the_repo_is_not_converted_again_on_the_next_boot() {
    let (pool, repo) = migrated_pool().await;

    let inserted = action_with_step(KIND_THE_DISCORD_CONVERTER_REWRITES);
    repo.save(&inserted).await.expect("insert");

    let updated = action_with_step(KIND_THE_DISCORD_CONVERTER_REWRITES);
    let updated_json = serde_json::to_string(&updated.sub_actions).expect("steps json");
    insert_legacy_row(&pool, updated.id, &updated_json).await;
    repo.save(&updated).await.expect("update over a legacy row");

    let duplicated = ActionId::new();
    repo.duplicate(inserted.id, duplicated, "copy")
        .await
        .expect("duplicate");

    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("boot conversion");

    for (id, path) in [
        (inserted.id, "insert"),
        (updated.id, "update"),
        (duplicated, "duplicate"),
    ] {
        assert_eq!(
            first_kind_id(&repo, id).await,
            KIND_THE_DISCORD_CONVERTER_REWRITES,
            "an action written by {path} was rewritten by the boot converter"
        );
    }
}

#[tokio::test]
async fn a_duplicate_of_an_unconverted_action_is_converted_with_its_source() {
    let (pool, repo) = migrated_pool().await;
    let source = ActionId::new();
    insert_legacy_row(&pool, source, r#"[{"Delay":{"ms":250}}]"#).await;
    let copy = ActionId::new();

    repo.duplicate(source, copy, "copy")
        .await
        .expect("duplicate");
    registry_migration::migrate_registry_format(&pool)
        .await
        .expect("boot conversion");

    assert_eq!(first_kind_id(&repo, copy).await, "core.logic.wait");
}
