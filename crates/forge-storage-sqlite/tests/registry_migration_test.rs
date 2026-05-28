#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage::DataProvider;
use forge_storage_sqlite::{SqliteBackend, apply_migrations, connect, registry_migration};
use forge_types::{ActionId, SubActionStep, Variant};

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

    assert_eq!(version, 1);

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
        .expect("second migrate — must not error");

    let (sub_actions_json, version): (String, i64) =
        sqlx::query_as("SELECT sub_actions, format_version FROM actions WHERE id = ?")
            .bind(action_id.to_string())
            .fetch_one(&pool)
            .await
            .expect("fetch action");

    assert_eq!(version, 1);

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

    let backend = SqliteBackend::open_with_key(&url, TEST_KEY)
        .await
        .expect("open backend");

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
