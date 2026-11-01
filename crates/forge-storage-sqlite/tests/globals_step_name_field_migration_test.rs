#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;

use std::str::FromStr;

use forge_storage::DataProvider;
use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use forge_types::{ActionId, Variant};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_RENAME: i64 = 74;
const RENAME_VERSION: i64 = 75;
const REGISTRY_FORMAT: i64 = 2;

struct StoredAction {
    id: &'static str,
    stored: &'static str,
    migrated: &'static str,
}

const ACTIONS: &[StoredAction] = &[
    StoredAction {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0A1",
        stored: r#"[{"kind_id":"core.globals.toggle","config":{"key":{"type":"string","value":"flag"}},"enabled":true}]"#,
        migrated: r#"[{"kind_id":"core.globals.toggle","config":{"name":{"type":"string","value":"flag"}},"enabled":true}]"#,
    },
    StoredAction {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0A2",
        stored: r#"[{"kind_id":"core.globals.decrement","config":{"key":{"type":"string","value":"lives"},"amount":{"type":"int","value":2}}},{"kind_id":"core.globals.array_append","config":{"key":{"type":"string","value":"queue"},"value":{"type":"string","value":"x"}}},{"kind_id":"core.globals.array_remove","config":{"key":{"type":"string","value":"queue"},"remove_all":{"type":"bool","value":true}}}]"#,
        migrated: r#"[{"kind_id":"core.globals.decrement","config":{"amount":{"type":"int","value":2},"name":{"type":"string","value":"lives"}}},{"kind_id":"core.globals.array_append","config":{"value":{"type":"string","value":"x"},"name":{"type":"string","value":"queue"}}},{"kind_id":"core.globals.array_remove","config":{"remove_all":{"type":"bool","value":true},"name":{"type":"string","value":"queue"}}}]"#,
    },
    StoredAction {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0A3",
        stored: r#"[{"kind_id":"core.logic.if_then_else","config":{"condition":{"type":"string","value":"true"},"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"core.globals.toggle"},"config":{"type":"object","value":{"key":{"type":"string","value":"deep"}}}}},{"type":"object","value":{"kind_id":{"type":"string","value":"core.logic.if_then_else"},"config":{"type":"object","value":{"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"core.globals.array_append"},"config":{"type":"object","value":{"key":{"type":"string","value":"deeper"},"value":{"type":"string","value":"y"}}}}}]}}}}}]}}}]"#,
        migrated: r#"[{"kind_id":"core.logic.if_then_else","config":{"condition":{"type":"string","value":"true"},"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"core.globals.toggle"},"config":{"type":"object","value":{"name":{"type":"string","value":"deep"}}}}},{"type":"object","value":{"kind_id":{"type":"string","value":"core.logic.if_then_else"},"config":{"type":"object","value":{"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"core.globals.array_append"},"config":{"type":"object","value":{"value":{"type":"string","value":"y"},"name":{"type":"string","value":"deeper"}}}}}]}}}}}]}}}]"#,
    },
    StoredAction {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0A4",
        stored: r#"[{"kind_id":"core.action.schedule","config":{"key":{"type":"string","value":"nightly"}}},{"kind_id":"core.globals.set","config":{"key":{"type":"string","value":"legacy"}}},{"kind_id":"core.globals.increment","config":{"name":{"type":"string","value":"hits"}}}]"#,
        migrated: r#"[{"kind_id":"core.action.schedule","config":{"key":{"type":"string","value":"nightly"}}},{"kind_id":"core.globals.set","config":{"key":{"type":"string","value":"legacy"}}},{"kind_id":"core.globals.increment","config":{"name":{"type":"string","value":"hits"}}}]"#,
    },
    StoredAction {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0A5",
        stored: r#"[{"kind_id":"core.globals.toggle","config":{"key":{"type":"string","value":"old"},"name":{"type":"string","value":"kept"}}}]"#,
        migrated: r#"[{"kind_id":"core.globals.toggle","config":{"key":{"type":"string","value":"old"},"name":{"type":"string","value":"kept"}}}]"#,
    },
    StoredAction {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0A6",
        stored: "not json at all",
        migrated: "not json at all",
    },
    StoredAction {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0A7",
        stored: "[]",
        migrated: "[]",
    },
];

fn db_url(dir: &TempDir) -> String {
    common::database_url(dir)
}

async fn schema_74_with_every_action(url: &str) -> SqlitePool {
    let pool = connect(url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_RENAME, &pool)
        .await
        .expect("the schema before the rename");
    for action in ACTIONS {
        sqlx::query(
            "INSERT INTO actions (id, sub_actions, name, queue_id, format_version) VALUES (?, ?, 'action', ?, ?)",
        )
        .bind(action.id)
        .bind(action.stored)
        .bind(common::DEFAULT_QUEUE)
        .bind(REGISTRY_FORMAT)
        .execute(&pool)
        .await
        .expect("a seeded action");
    }
    pool
}

async fn drifted_actions(pool: &SqlitePool) -> Vec<String> {
    let mut drifted = Vec::new();
    for action in ACTIONS {
        let found: String = sqlx::query_scalar("SELECT sub_actions FROM actions WHERE id = ?")
            .bind(action.id)
            .fetch_one(pool)
            .await
            .expect("the seeded action");
        if found != action.migrated {
            drifted.push(format!(
                "{}:\n  expected: {}\n  found:    {found}",
                action.id, action.migrated
            ));
        }
    }
    drifted
}

async fn journal_entries(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = ?")
        .bind(RENAME_VERSION)
        .fetch_one(pool)
        .await
        .expect("the journal")
}

#[tokio::test]
async fn migration_renames_only_the_globals_name_field_to_the_golden_bytes() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = schema_74_with_every_action(&db_url(&dir)).await;

    apply_migrations(&pool).await.expect("migrate");

    let drifted = drifted_actions(&pool).await;
    assert!(
        drifted.is_empty(),
        "actions drifted:\n{}",
        drifted.join("\n")
    );
}

#[tokio::test]
async fn a_second_migration_run_changes_nothing_and_journals_the_rename_once() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = schema_74_with_every_action(&db_url(&dir)).await;
    apply_migrations(&pool).await.expect("first migrate");

    apply_migrations(&pool).await.expect("second migrate");

    assert_eq!(
        (drifted_actions(&pool).await, journal_entries(&pool).await),
        (Vec::<String>::new(), 1)
    );
}

#[tokio::test]
async fn a_migrated_toggle_step_opens_through_the_backend_with_its_name_field() {
    let dir = TempDir::new().expect("a temporary directory");
    let url = db_url(&dir);
    schema_74_with_every_action(&url).await.close().await;

    let backend = common::sandboxed_backend_at(&url, common::TEST_KEY).await;

    let action = backend
        .action_repo()
        .get(ActionId::from_str(ACTIONS[0].id).expect("an action id"))
        .await
        .expect("the action")
        .expect("the seeded action");
    let config = &action.sub_actions[0].config;
    assert_eq!(
        (config.get("name").cloned(), config.contains_key("key")),
        (Some(Variant::String("flag".to_owned())), false)
    );
}
