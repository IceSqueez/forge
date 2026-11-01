#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;

use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_RENAME: i64 = 75;
const RENAME_VERSION: i64 = 76;
const REGISTRY_FORMAT: i64 = 2;

const RETIRED_TRIGGER: &str = "obs.virtualcam.state_changed";
const CURRENT_TRIGGER: &str = "obs.virtualcam.status_changed";
const FAVORITES_KEY: &str = "picker.favorites.triggers";

const KICK_BAN_ACTION: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0B1";
const KICK_BAN_CALLEE: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0B2";
const UNBOUND_ACTION: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0B3";
const YOUTUBE_BAN_ACTION: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0B4";

struct StoredAction {
    id: &'static str,
    stored: String,
    migrated: String,
}

fn kick_ban_stored() -> String {
    format!(
        r#"[{{"kind_id":"core.log.write","config":{{"message":{{"type":"string","value":"banned for %duration_secs% s, %duration_seconds% kept"}}}}}},{{"kind_id":"core.action.run","config":{{"action":{{"type":"string","value":"{KICK_BAN_CALLEE}"}}}}}}]"#
    )
}

fn kick_ban_migrated() -> String {
    format!(
        r#"[{{"kind_id":"core.log.write","config":{{"message":{{"type":"string","value":"banned for %duration_seconds% s, %duration_seconds% kept"}}}}}},{{"kind_id":"core.action.run","config":{{"action":{{"type":"string","value":"{KICK_BAN_CALLEE}"}}}}}}]"#
    )
}

fn actions() -> Vec<StoredAction> {
    vec![
        StoredAction {
            id: "01JBZ8Q4T5N7W3K2M9X6V1R0A1",
            stored: r#"[{"kind_id":"overlay.send","config":{"overlay":{"type":"string","value":"alerts"},"duration_secs":{"type":"int","value":7}},"enabled":true}]"#.to_owned(),
            migrated: r#"[{"kind_id":"overlay.send","config":{"overlay":{"type":"string","value":"alerts"},"duration_seconds":{"type":"int","value":7}},"enabled":true}]"#.to_owned(),
        },
        StoredAction {
            id: "01JBZ8Q4T5N7W3K2M9X6V1R0A2",
            stored: r#"[{"kind_id":"youtube.stream.insert_ad_break","config":{"duration_secs":{"type":"int","value":45}},"enabled":true},{"kind_id":"core.logic.if_then_else","config":{"condition":{"type":"string","value":"%obs.source.is_enabled%"},"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"overlay.send"},"config":{"type":"object","value":{"duration_secs":{"type":"int","value":3}}}}},{"type":"object","value":{"kind_id":{"type":"string","value":"core.logic.if_then_else"},"config":{"type":"object","value":{"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"youtube.stream.insert_ad_break"},"config":{"type":"object","value":{"duration_secs":{"type":"int","value":9}}}}}]}}}}}]}}}]"#.to_owned(),
            migrated: r#"[{"kind_id":"youtube.stream.insert_ad_break","config":{"duration_seconds":{"type":"int","value":45}},"enabled":true},{"kind_id":"core.logic.if_then_else","config":{"condition":{"type":"string","value":"%obs.source.is_visible%"},"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"overlay.send"},"config":{"type":"object","value":{"duration_seconds":{"type":"int","value":3}}}}},{"type":"object","value":{"kind_id":{"type":"string","value":"core.logic.if_then_else"},"config":{"type":"object","value":{"then_chain":{"type":"array","value":[{"type":"object","value":{"kind_id":{"type":"string","value":"youtube.stream.insert_ad_break"},"config":{"type":"object","value":{"duration_seconds":{"type":"int","value":9}}}}}]}}}}}]}}}]"#.to_owned(),
        },
        StoredAction {
            id: "01JBZ8Q4T5N7W3K2M9X6V1R0A3",
            stored: r#"[{"kind_id":"overlay.send","config":{"duration_secs":{"type":"int","value":1},"duration_seconds":{"type":"int","value":2}}},{"kind_id":"core.log.write","config":{"duration_secs":{"type":"int","value":5}}},{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"%obs.filter.is_enabled% %duration_secs%"}}}]"#.to_owned(),
            migrated: r#"[{"kind_id":"overlay.send","config":{"duration_secs":{"type":"int","value":1},"duration_seconds":{"type":"int","value":2}}},{"kind_id":"core.log.write","config":{"duration_secs":{"type":"int","value":5}}},{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"%obs.filter.is_enabled% %duration_secs%"}}}]"#.to_owned(),
        },
        StoredAction {
            id: KICK_BAN_ACTION,
            stored: kick_ban_stored(),
            migrated: kick_ban_migrated(),
        },
        StoredAction {
            id: KICK_BAN_CALLEE,
            stored: r#"[{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"%duration_secs%"}}}]"#.to_owned(),
            migrated: r#"[{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"%duration_seconds%"}}}]"#.to_owned(),
        },
        StoredAction {
            id: UNBOUND_ACTION,
            stored: r#"[{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"%duration_secs%"}}}]"#.to_owned(),
            migrated: r#"[{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"%duration_secs%"}}}]"#.to_owned(),
        },
        StoredAction {
            id: YOUTUBE_BAN_ACTION,
            stored: r#"[{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"%duration_secs%%duration_secs%"}}}]"#.to_owned(),
            migrated: r#"[{"kind_id":"core.log.write","config":{"message":{"type":"string","value":"%duration_seconds%%duration_seconds%"}}}]"#.to_owned(),
        },
        StoredAction {
            id: "01JBZ8Q4T5N7W3K2M9X6V1R0A8",
            stored: "not json at all %obs.source.is_enabled%".to_owned(),
            migrated: "not json at all %obs.source.is_visible%".to_owned(),
        },
        StoredAction {
            id: "01JBZ8Q4T5N7W3K2M9X6V1R0A9",
            stored: "[]".to_owned(),
            migrated: "[]".to_owned(),
        },
    ]
}

struct StoredTrigger {
    id: &'static str,
    kind: &'static str,
    user_defined: i64,
    overrides: &'static str,
    migrated_kind: &'static str,
    migrated_overrides: &'static str,
}

const TRIGGERS: &[StoredTrigger] = &[
    StoredTrigger {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0T1",
        kind: RETIRED_TRIGGER,
        user_defined: 0,
        overrides: "{}",
        migrated_kind: CURRENT_TRIGGER,
        migrated_overrides: "{}",
    },
    StoredTrigger {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0T2",
        kind: RETIRED_TRIGGER,
        user_defined: 1,
        overrides: r#"{"note":{"type":"string","value":"%obs.source.is_enabled%"}}"#,
        migrated_kind: CURRENT_TRIGGER,
        migrated_overrides: r#"{"note":{"type":"string","value":"%obs.source.is_visible%"}}"#,
    },
    StoredTrigger {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0T3",
        kind: "obs.virtualcam.started",
        user_defined: 0,
        overrides: "{}",
        migrated_kind: "obs.virtualcam.started",
        migrated_overrides: "{}",
    },
    StoredTrigger {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0T4",
        kind: "kick.moderation.banned",
        user_defined: 1,
        overrides: "{}",
        migrated_kind: "kick.moderation.banned",
        migrated_overrides: "{}",
    },
    StoredTrigger {
        id: "01JBZ8Q4T5N7W3K2M9X6V1R0T5",
        kind: "youtube.channel.user_banned",
        user_defined: 1,
        overrides: "{}",
        migrated_kind: "youtube.channel.user_banned",
        migrated_overrides: "{}",
    },
];

const BINDINGS: &[(&str, &str)] = &[
    (KICK_BAN_ACTION, "01JBZ8Q4T5N7W3K2M9X6V1R0T4"),
    (YOUTUBE_BAN_ACTION, "01JBZ8Q4T5N7W3K2M9X6V1R0T5"),
];

const FAVORITES_STORED: &str = r#"["twitch.chat.message","obs.virtualcam.state_changed"]"#;
const FAVORITES_MIGRATED: &str = r#"["twitch.chat.message","obs.virtualcam.status_changed"]"#;

fn db_url(dir: &TempDir) -> String {
    common::database_url(dir)
}

async fn schema_75_with_every_row(url: &str) -> SqlitePool {
    let pool = connect(url).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_RENAME, &pool)
        .await
        .expect("the schema before the rename");
    for action in actions() {
        sqlx::query(
            "INSERT INTO actions (id, sub_actions, name, queue_id, format_version) VALUES (?, ?, 'action', ?, ?)",
        )
        .bind(action.id)
        .bind(&action.stored)
        .bind(common::DEFAULT_QUEUE)
        .bind(REGISTRY_FORMAT)
        .execute(&pool)
        .await
        .expect("a seeded action");
    }
    for trigger in TRIGGERS {
        sqlx::query(
            "INSERT INTO trigger_instances (id, kind_id, name, overrides, user_defined) VALUES (?, ?, 'trigger', ?, ?)",
        )
        .bind(trigger.id)
        .bind(trigger.kind)
        .bind(trigger.overrides)
        .bind(trigger.user_defined)
        .execute(&pool)
        .await
        .expect("a seeded trigger");
    }
    for (position, (action_id, trigger_id)) in (0_i64..).zip(BINDINGS) {
        sqlx::query(
            "INSERT INTO action_trigger_instances (action_id, trigger_instance_id, position) VALUES (?, ?, ?)",
        )
        .bind(action_id)
        .bind(trigger_id)
        .bind(position)
        .execute(&pool)
        .await
        .expect("a seeded binding");
    }
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(FAVORITES_KEY)
    .bind(FAVORITES_STORED)
    .execute(&pool)
    .await
    .expect("seeded favorites");
    pool
}

async fn drifted_rows(pool: &SqlitePool) -> Vec<String> {
    let mut drifted = Vec::new();
    for action in actions() {
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
    for trigger in TRIGGERS {
        let found: (String, String) =
            sqlx::query_as("SELECT kind_id, overrides FROM trigger_instances WHERE id = ?")
                .bind(trigger.id)
                .fetch_one(pool)
                .await
                .expect("the seeded trigger");
        let expected = (
            trigger.migrated_kind.to_owned(),
            trigger.migrated_overrides.to_owned(),
        );
        if found != expected {
            drifted.push(format!(
                "{}:\n  expected: {expected:?}\n  found:    {found:?}",
                trigger.id
            ));
        }
    }
    let favorites: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(FAVORITES_KEY)
        .fetch_one(pool)
        .await
        .expect("the favorites");
    if favorites != FAVORITES_MIGRATED {
        drifted.push(format!("favorites: {favorites}"));
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
async fn migration_renames_the_ids_tokens_and_fields_to_the_golden_bytes() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = schema_75_with_every_row(&db_url(&dir)).await;

    apply_migrations(&pool).await.expect("migrate");

    let drifted = drifted_rows(&pool).await;
    assert!(drifted.is_empty(), "rows drifted:\n{}", drifted.join("\n"));
}

#[tokio::test]
async fn a_second_migration_run_changes_nothing_and_journals_the_rename_once() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = schema_75_with_every_row(&db_url(&dir)).await;
    apply_migrations(&pool).await.expect("first migrate");

    apply_migrations(&pool).await.expect("second migrate");

    assert_eq!(
        (drifted_rows(&pool).await, journal_entries(&pool).await),
        (Vec::<String>::new(), 1)
    );
}

#[tokio::test]
async fn replaying_the_migration_statements_over_migrated_data_changes_nothing() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = schema_75_with_every_row(&db_url(&dir)).await;
    apply_migrations(&pool).await.expect("migrate");
    let migration = MIGRATIONS
        .iter()
        .find(|migration| migration.version == RENAME_VERSION)
        .expect("the rename migration");

    sqlx::raw_sql(migration.sql.clone())
        .execute(&pool)
        .await
        .expect("the replayed statements");

    assert_eq!(drifted_rows(&pool).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_default_trigger_keeps_its_retired_id_when_the_current_default_already_exists() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = connect(&db_url(&dir)).await.expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_RENAME, &pool)
        .await
        .expect("the schema before the rename");
    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, user_defined) VALUES
         ('01JBZ8Q4T5N7W3K2M9X6V1R0C1', ?, 'old', 0),
         ('01JBZ8Q4T5N7W3K2M9X6V1R0C2', ?, 'new', 0)",
    )
    .bind(RETIRED_TRIGGER)
    .bind(CURRENT_TRIGGER)
    .execute(&pool)
    .await
    .expect("two default triggers");

    apply_migrations(&pool).await.expect("migrate");

    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind_id FROM trigger_instances ORDER BY id")
            .fetch_all(&pool)
            .await
            .expect("the triggers");
    assert_eq!(kinds, [RETIRED_TRIGGER, CURRENT_TRIGGER]);
}
