#![allow(clippy::expect_used, clippy::panic)]
mod common;

use std::collections::BTreeSet;

use forge_storage::DataProvider;
use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_RENAME: i64 = 57;
const RENAME_VERSION: i64 = 58;
const QUEUE_ID: &str = "00000000000000000000000000";
const REGISTRY_FORMAT: i64 = 2;
const STARTED_AT: i64 = 1_761_328_800;

struct StoredRow {
    table: &'static str,
    key_column: &'static str,
    key: &'static str,
    column: &'static str,
    stored: &'static str,
    migrated: &'static str,
}

const TRIGGER_RETIRED: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0T1";
const TRIGGER_NAMING_A_STEP_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0T2";
const TRIGGER_UNRELATED: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0T3";
const ACTION_RETIRED: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0A1";
const ACTION_TRIGGER_ID_AS_STEP: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0A2";
const ACTION_UNREADABLE: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0A3";
const ACTION_UNRELATED: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0A4";
const HISTORY_RETIRED: &str = "1";
const HISTORY_CROSSED_STORES: &str = "2";
const HISTORY_UNRELATED: &str = "3";
const EVENT_NAMING_RETIRED_IDS: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0E1";

const ROWS: &[StoredRow] = &[
    StoredRow {
        table: "trigger_instances",
        key_column: "id",
        key: TRIGGER_RETIRED,
        column: "kind_id",
        stored: "obs.record.started",
        migrated: "obs.recording.started",
    },
    StoredRow {
        table: "trigger_instances",
        key_column: "id",
        key: TRIGGER_NAMING_A_STEP_ID,
        column: "kind_id",
        stored: "obs.record.start",
        migrated: "obs.record.start",
    },
    StoredRow {
        table: "trigger_instances",
        key_column: "id",
        key: TRIGGER_UNRELATED,
        column: "kind_id",
        stored: "twitch.chat.message",
        migrated: "twitch.chat.message",
    },
    StoredRow {
        table: "actions",
        key_column: "id",
        key: ACTION_RETIRED,
        column: "sub_actions",
        stored: r#"[{"kind_id":"obs.scenes.switch_current","config":{"note":{"type":"string","value":"obs.scenes.switch_current"},"scene":{"type":"string","value":"Сцена"}},"enabled":true,"continue_on_error":false,"condition":null,"label":"go"},{"kind_id":"core.logic.if_then_else","config":{"condition":{"type":"string","value":"true"},"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"obs.record.start"},"x_extra":{"type":"int","value":1}}},{"type":"object","value":{"config":{"type":"object","value":{"text":{"type":"string","value":"obs.stream.stop"}}},"enabled":{"type":"bool","value":false},"kind_id":{"type":"string","value":"obs.stream.stop"}}}]}},"enabled":true,"continue_on_error":false,"condition":null,"label":null}]"#,
        migrated: r#"[{"kind_id":"obs.scene.switch_current","config":{"note":{"type":"string","value":"obs.scenes.switch_current"},"scene":{"type":"string","value":"Сцена"}},"enabled":true,"continue_on_error":false,"condition":null,"label":"go"},{"kind_id":"core.logic.if_then_else","config":{"condition":{"type":"string","value":"true"},"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"obs.recording.start"},"x_extra":{"type":"int","value":1}}},{"type":"object","value":{"config":{"type":"object","value":{"text":{"type":"string","value":"obs.stream.stop"}}},"enabled":{"type":"bool","value":false},"kind_id":{"type":"string","value":"obs.streaming.stop"}}}]}},"enabled":true,"continue_on_error":false,"condition":null,"label":null}]"#,
    },
    StoredRow {
        table: "actions",
        key_column: "id",
        key: ACTION_TRIGGER_ID_AS_STEP,
        column: "sub_actions",
        stored: r#"[{"kind_id":"obs.record.started","config":{"message":{"type":"string","value":"obs.record.start"}}}]"#,
        migrated: r#"[{"kind_id":"obs.record.started","config":{"message":{"type":"string","value":"obs.record.start"}}}]"#,
    },
    StoredRow {
        table: "actions",
        key_column: "id",
        key: ACTION_UNREADABLE,
        column: "sub_actions",
        stored: r#"[{"kind_id":"obs.record.start","config":{},"enabled":"yes"}]"#,
        migrated: r#"[{"kind_id":"obs.record.start","config":{},"enabled":"yes"}]"#,
    },
    StoredRow {
        table: "actions",
        key_column: "id",
        key: ACTION_UNRELATED,
        column: "sub_actions",
        stored: r#"[{"kind_id":"core.log.write","config":{}}]"#,
        migrated: r#"[{"kind_id":"core.log.write","config":{}}]"#,
    },
    StoredRow {
        table: "action_history",
        key_column: "id",
        key: HISTORY_RETIRED,
        column: "context",
        stored: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A1","metadata":{"kind":"trigger","event_id":"01JBZ8Q4T5N7W3K2M9X6V1R0E1","trigger_kind":"obs.stream.started"},"arg_stack_snapshot":{"said":{"type":"string","value":"obs.stream.start"}},"started_at":"2025-10-24T18:00:00Z","completed_at":"2025-10-24T18:00:01Z","telemetry":[{"index":0,"kind":"obs.stream.start","started_at":"2025-10-24T18:00:00Z","duration_ms":12,"outcome":"Success"},{"index":18446744073709551615,"kind":"obs.record.stop","started_at":"2025-10-24T18:00:00Z","duration_ms":3,"outcome":{"Failed":"obs.record.stop"},"args_in":{"why":"obs.record.stop"}}],"outcome":"Success"}"#,
        migrated: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A1","metadata":{"kind":"trigger","event_id":"01JBZ8Q4T5N7W3K2M9X6V1R0E1","trigger_kind":"obs.streaming.started"},"arg_stack_snapshot":{"said":{"type":"string","value":"obs.stream.start"}},"started_at":"2025-10-24T18:00:00Z","completed_at":"2025-10-24T18:00:01Z","telemetry":[{"index":0,"kind":"obs.streaming.start","started_at":"2025-10-24T18:00:00Z","duration_ms":12,"outcome":"Success"},{"index":18446744073709551615,"kind":"obs.recording.stop","started_at":"2025-10-24T18:00:00Z","duration_ms":3,"outcome":{"Failed":"obs.record.stop"},"args_in":{"why":"obs.record.stop"}}],"outcome":"Success"}"#,
    },
    StoredRow {
        table: "action_history",
        key_column: "id",
        key: HISTORY_CROSSED_STORES,
        column: "context",
        stored: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A2","metadata":{"kind":"trigger","event_id":"01JBZ8Q4T5N7W3K2M9X6V1R0E1","trigger_kind":"obs.record.start"},"arg_stack_snapshot":{},"started_at":"2025-10-24T18:00:00Z","completed_at":null,"telemetry":[{"index":0,"kind":"obs.record.started","started_at":"2025-10-24T18:00:00Z","duration_ms":1,"outcome":"Success"}],"outcome":"Cancelled"}"#,
        migrated: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A2","metadata":{"kind":"trigger","event_id":"01JBZ8Q4T5N7W3K2M9X6V1R0E1","trigger_kind":"obs.record.start"},"arg_stack_snapshot":{},"started_at":"2025-10-24T18:00:00Z","completed_at":null,"telemetry":[{"index":0,"kind":"obs.record.started","started_at":"2025-10-24T18:00:00Z","duration_ms":1,"outcome":"Success"}],"outcome":"Cancelled"}"#,
    },
    StoredRow {
        table: "action_history",
        key_column: "id",
        key: HISTORY_UNRELATED,
        column: "context",
        stored: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A4", "metadata":{"kind":"quick_action","builtin_id":"obs","label":"Go"},"arg_stack_snapshot":{},"started_at":"2025-10-24T18:00:00Z","telemetry":[],"outcome":"Success"}"#,
        migrated: r#"{"action_id":"01JBZ8Q4T5N7W3K2M9X6V1R0A4", "metadata":{"kind":"quick_action","builtin_id":"obs","label":"Go"},"arg_stack_snapshot":{},"started_at":"2025-10-24T18:00:00Z","telemetry":[],"outcome":"Success"}"#,
    },
    StoredRow {
        table: "settings",
        key_column: "key",
        key: "picker.favorites.sub_actions",
        column: "value",
        stored: r#"["core.logic.wait","obs.scenes.switch_current","obs.scene_collection.switch"]"#,
        migrated: r#"["core.logic.wait","obs.scene.switch_current","obs.collection.switch"]"#,
    },
    StoredRow {
        table: "settings",
        key_column: "key",
        key: "picker.favorites.triggers",
        column: "value",
        stored: r#"["obs.scenes.current_changed", "obs.record.start", "twitch.chat.message"]"#,
        migrated: r#"["obs.scene.changed","obs.record.start","twitch.chat.message"]"#,
    },
    StoredRow {
        table: "settings",
        key_column: "key",
        key: "ui.note",
        column: "value",
        stored: r#"["obs.scenes.switch_current"]"#,
        migrated: r#"["obs.scenes.switch_current"]"#,
    },
    StoredRow {
        table: "event_log",
        key_column: "id",
        key: EVENT_NAMING_RETIRED_IDS,
        column: "payload",
        stored: r#"{"trigger":"obs.record.started","step":"obs.record.start"}"#,
        migrated: r#"{"trigger":"obs.record.started","step":"obs.record.start"}"#,
    },
];

fn db_url(dir: &TempDir) -> String {
    format!("sqlite://{}", dir.path().join("forge.db").display())
}

async fn insert(pool: &SqlitePool, row: &StoredRow) {
    let statement = match row.table {
        "trigger_instances" => {
            "INSERT INTO trigger_instances (id, kind_id, name, user_defined) VALUES (?, ?, 'trigger', 1)"
        }
        "actions" => {
            "INSERT INTO actions (id, sub_actions, name, queue_id, format_version) VALUES (?, ?, 'action', ?, ?)"
        }
        "action_history" => {
            "INSERT INTO action_history (id, context, action_id, started_at, duration_ms, outcome) VALUES (?, ?, 'a', ?, 1, 'success')"
        }
        "settings" => "INSERT INTO settings (key, value) VALUES (?, ?)",
        "event_log" => {
            "INSERT INTO event_log (id, payload, source, kind, timestamp) VALUES (?, ?, 'obs', 'obs.recording.started', ?)"
        }
        other => panic!("no insert for {other}"),
    };
    let query = sqlx::query(statement).bind(row.key).bind(row.stored);
    let query = match row.table {
        "actions" => query.bind(QUEUE_ID).bind(REGISTRY_FORMAT),
        "action_history" | "event_log" => query.bind(STARTED_AT),
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

async fn schema_57_with_every_store(url: &str) -> SqlitePool {
    let pool = connect(url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_RENAME, &pool)
        .await
        .expect("the schema before the rename");
    for row in ROWS {
        insert(&pool, row).await;
    }
    pool
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
        .bind(RENAME_VERSION)
        .fetch_one(pool)
        .await
        .expect("the journal")
}

#[tokio::test]
async fn migration_rewrites_only_retired_kind_ids_to_the_golden_bytes_in_every_store() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = schema_57_with_every_store(&db_url(&dir)).await;

    apply_migrations(&pool).await.expect("migrate");

    let drifted = drifted_rows(&pool).await;
    assert!(drifted.is_empty(), "rows drifted:\n{}", drifted.join("\n"));
}

#[tokio::test]
async fn a_second_migration_run_leaves_every_row_and_the_journal_as_the_first_left_them() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = schema_57_with_every_store(&db_url(&dir)).await;
    apply_migrations(&pool).await.expect("first migrate");

    apply_migrations(&pool).await.expect("second migrate");

    assert_eq!(
        (
            drifted_rows(&pool).await,
            rename_journal_entries(&pool).await
        ),
        (Vec::<String>::new(), 1)
    );
}

#[tokio::test]
async fn a_schema_57_database_file_opens_through_the_backend_with_its_trigger_kinds_renamed() {
    let dir = TempDir::new().expect("a temporary directory");
    let url = db_url(&dir);
    schema_57_with_every_store(&url).await.close().await;

    let backend = common::sandboxed_backend(&url, common::TEST_KEY).await;

    let kinds: BTreeSet<String> = backend
        .trigger_instance_repo()
        .list_all()
        .await
        .expect("trigger instances")
        .into_iter()
        .map(|trigger| trigger.kind_id)
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from(
            [
                "obs.recording.started",
                "obs.record.start",
                "twitch.chat.message"
            ]
            .map(str::to_owned)
        )
    );
}

#[tokio::test]
async fn a_failing_rename_rolls_back_every_store_and_records_nothing() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = schema_57_with_every_store(&db_url(&dir)).await;
    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, user_defined)
         VALUES ('01JBZ8Q4T5N7W3K2M9X6V1R0T8', 'obs.scene.changed', 'default', 0),
                ('01JBZ8Q4T5N7W3K2M9X6V1R0T9', 'obs.scenes.current_changed', 'default', 0)",
    )
    .execute(&pool)
    .await
    .expect("a default instance under both spellings");

    let result = apply_migrations(&pool).await;

    let untouched: Vec<bool> = {
        let mut untouched = Vec::new();
        for row in ROWS {
            untouched.push(stored_value(&pool, row).await == row.stored);
        }
        untouched
    };
    assert_eq!(
        (
            result.is_err(),
            untouched.iter().all(|same| *same),
            rename_journal_entries(&pool).await
        ),
        (true, true, 0)
    );
}
