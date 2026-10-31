#![allow(clippy::expect_used, clippy::panic)]

mod common;
use forge_storage_sqlite::migrations::apply;
use forge_storage_sqlite::{MIGRATIONS, connect};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tempfile::TempDir;

const OBS_VERSION_BEFORE_RENAME: i64 = 57;
const VTUBE_VERSION_BEFORE_RENAME: i64 = 66;
const VTUBE_RENAME_VERSION: i64 = 67;
const QUEUE_ID: &str = "00000000000000000000000000";
const REGISTRY_FORMAT: i64 = 2;
const STARTED_AT: i64 = 1_761_328_800;
const SUB_ACTION_FAVORITES: &str = "picker.favorites.sub_actions";

struct StoredRow {
    table: &'static str,
    key_column: &'static str,
    key: &'static str,
    column: &'static str,
    stored: &'static str,
    migrated: &'static str,
}

const ROWS: &[StoredRow] = &[
    StoredRow {
        table: "actions",
        key_column: "id",
        key: "01JBZ8Q4T5N7W3K2M9X6V1R0A1",
        column: "sub_actions",
        stored: r#"[{"kind_id":"vtube.params.reset","config":{"note":{"type":"string","value":"vtube.params.reset"}},"enabled":true,"continue_on_error":false,"condition":null,"label":"top"},{"kind_id":"core.logic.if_then_else","config":{"condition":{"type":"string","value":"true"},"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"vtube.params.reset"},"x_extra":{"type":"int","value":1}}}]}},"enabled":true,"continue_on_error":false,"condition":null,"label":null}]"#,
        migrated: r#"[{"kind_id":"vtube.expressions.reset","config":{"note":{"type":"string","value":"vtube.params.reset"}},"enabled":true,"continue_on_error":false,"condition":null,"label":"top"},{"kind_id":"core.logic.if_then_else","config":{"condition":{"type":"string","value":"true"},"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"vtube.expressions.reset"},"x_extra":{"type":"int","value":1}}}]}},"enabled":true,"continue_on_error":false,"condition":null,"label":null}]"#,
    },
    StoredRow {
        table: "actions",
        key_column: "id",
        key: "01JBZ8Q4T5N7W3K2M9X6V1R0A2",
        column: "sub_actions",
        stored: r#"[{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"vtube.params.reset"}}}]"#,
        migrated: r#"[{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"vtube.params.reset"}}}]"#,
    },
    StoredRow {
        table: "actions",
        key_column: "id",
        key: "01JBZ8Q4T5N7W3K2M9X6V1R0A3",
        column: "sub_actions",
        stored: r#"[{"kind_id":"vtube.params.set","config":{}}]"#,
        migrated: r#"[{"kind_id":"vtube.params.set","config":{}}]"#,
    },
    StoredRow {
        table: "action_history",
        key_column: "id",
        key: "1",
        column: "context",
        stored: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A1","metadata":{"kind":"trigger","trigger_kind":"vtube.params.reset"},"arg_stack_snapshot":{"said":{"type":"string","value":"vtube.params.reset"}},"telemetry":[{"index":0,"kind":"vtube.params.reset","duration_ms":12,"outcome":"Success","args_in":{"why":"vtube.params.reset"}},{"index":1,"kind":"core.log.write","duration_ms":1,"outcome":"Success"}],"outcome":"Success"}"#,
        migrated: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A1","metadata":{"kind":"trigger","trigger_kind":"vtube.params.reset"},"arg_stack_snapshot":{"said":{"type":"string","value":"vtube.params.reset"}},"telemetry":[{"index":0,"kind":"vtube.expressions.reset","duration_ms":12,"outcome":"Success","args_in":{"why":"vtube.params.reset"}},{"index":1,"kind":"core.log.write","duration_ms":1,"outcome":"Success"}],"outcome":"Success"}"#,
    },
    StoredRow {
        table: "action_history",
        key_column: "id",
        key: "2",
        column: "context",
        stored: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A2", "metadata":{"kind":"quick_action","builtin_id":"vtube","label":"Go"},"telemetry":[],"outcome":"Success"}"#,
        migrated: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A2", "metadata":{"kind":"quick_action","builtin_id":"vtube","label":"Go"},"telemetry":[],"outcome":"Success"}"#,
    },
    StoredRow {
        table: "settings",
        key_column: "key",
        key: SUB_ACTION_FAVORITES,
        column: "value",
        stored: r#"["core.logic.wait", "vtube.params.reset"]"#,
        migrated: r#"["core.logic.wait","vtube.expressions.reset"]"#,
    },
    StoredRow {
        table: "settings",
        key_column: "key",
        key: "picker.favorites.triggers",
        column: "value",
        stored: r#"["vtube.params.reset", "twitch.chat.message"]"#,
        migrated: r#"["vtube.params.reset", "twitch.chat.message"]"#,
    },
    StoredRow {
        table: "settings",
        key_column: "key",
        key: "ui.note",
        column: "value",
        stored: r#"["vtube.params.reset"]"#,
        migrated: r#"["vtube.params.reset"]"#,
    },
];

fn db_url(dir: &TempDir) -> String {
    common::database_url(dir)
}

async fn insert(pool: &SqlitePool, row: &StoredRow) {
    let statement = match row.table {
        "actions" => {
            "INSERT INTO actions (id, sub_actions, name, queue_id, format_version) VALUES (?, ?, 'action', ?, ?)"
        }
        "action_history" => {
            "INSERT INTO action_history (id, context, action_id, started_at, duration_ms, outcome) VALUES (?, ?, 'a', ?, 1, 'success')"
        }
        "settings" => "INSERT INTO settings (key, value) VALUES (?, ?)",
        other => panic!("no insert for {other}"),
    };
    let query = sqlx::query(statement).bind(row.key).bind(row.stored);
    let query = match row.table {
        "actions" => query.bind(QUEUE_ID).bind(REGISTRY_FORMAT),
        "action_history" => query.bind(STARTED_AT),
        _ => query,
    };
    query.execute(pool).await.expect("a seeded row");
}

async fn stored_value(pool: &SqlitePool, row: &StoredRow) -> String {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM {} WHERE {} = ?",
        row.column, row.table, row.key_column
    )))
    .bind(row.key)
    .fetch_one(pool)
    .await
    .expect("the seeded row")
}

async fn seeded_before_the_vtube_rename(pool: &SqlitePool) {
    MIGRATIONS
        .run_to(VTUBE_VERSION_BEFORE_RENAME, pool)
        .await
        .expect("the schema before the rename");
    for row in ROWS {
        insert(pool, row).await;
    }
}

async fn drifted_rows(pool: &SqlitePool) -> Vec<String> {
    let mut drifted = Vec::new();
    for row in ROWS {
        let found = stored_value(pool, row).await;
        if found != row.migrated {
            drifted.push(format!(
                "{} {}:\n  expected: {}\n  found:    {found}",
                row.table, row.key, row.migrated
            ));
        }
    }
    drifted
}

async fn rename_journal_entries(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = ?")
        .bind(VTUBE_RENAME_VERSION)
        .fetch_one(pool)
        .await
        .expect("the journal")
}

async fn favorites(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(SUB_ACTION_FAVORITES)
        .fetch_one(pool)
        .await
        .expect("the favorites")
}

#[tokio::test]
async fn migration_rewrites_only_the_retired_vtube_step_id_to_the_golden_bytes_in_every_store() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = connect(&db_url(&dir)).await.expect("a database");
    seeded_before_the_vtube_rename(&pool).await;

    apply(&pool).await.expect("migrate");

    let drifted = drifted_rows(&pool).await;
    assert!(drifted.is_empty(), "rows drifted:\n{}", drifted.join("\n"));
}

#[tokio::test]
async fn a_second_run_rewrites_nothing_and_records_the_rename_once() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = connect(&db_url(&dir)).await.expect("a database");
    seeded_before_the_vtube_rename(&pool).await;
    apply(&pool).await.expect("first migrate");
    let saved_after_the_rename = r#"["vtube.params.reset"]"#;
    sqlx::query("UPDATE settings SET value = ? WHERE key = ?")
        .bind(saved_after_the_rename)
        .bind(SUB_ACTION_FAVORITES)
        .execute(&pool)
        .await
        .expect("a favorites write after the rename");

    apply(&pool).await.expect("second migrate");

    assert_eq!(
        (favorites(&pool).await, rename_journal_entries(&pool).await),
        (saved_after_the_rename.to_owned(), 1)
    );
}

#[tokio::test]
async fn both_renames_run_in_sequence_on_a_single_connection() {
    let dir = TempDir::new().expect("a temporary directory");
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("forge.db"))
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("a one-connection pool");
    MIGRATIONS
        .run_to(OBS_VERSION_BEFORE_RENAME, &pool)
        .await
        .expect("the schema before both renames");
    sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
        .bind(SUB_ACTION_FAVORITES)
        .bind(r#"["obs.scenes.switch_current","vtube.params.reset"]"#)
        .execute(&pool)
        .await
        .expect("favorites naming both retired ids");

    apply(&pool).await.expect("migrate");

    assert_eq!(
        favorites(&pool).await,
        r#"["obs.scene.switch_current","vtube.expressions.reset"]"#
    );
}
