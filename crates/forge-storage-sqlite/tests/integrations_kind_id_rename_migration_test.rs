#![allow(clippy::expect_used, clippy::unwrap_used)]

use forge_storage_sqlite::{apply_migrations, connect};
use sqlx::SqlitePool;

const MIGRATION_SQL: &str = include_str!("../migrations/0020_integrations_kind_id_rename.sql");

async fn fresh_pool() -> SqlitePool {
    let pool = connect("sqlite::memory:").await.expect("connect");
    apply_migrations(&pool).await.expect("apply migrations");
    pool
}

async fn run_migration(pool: &SqlitePool) {
    sqlx::raw_sql(MIGRATION_SQL)
        .execute(pool)
        .await
        .expect("run 0020 migration sql");
}

async fn seed(pool: &SqlitePool, id: &str, kind_id: &str, user_defined: i64) {
    sqlx::query(
        "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined) \
         VALUES (?, ?, 'seed', '{}', 1, ?)",
    )
    .bind(id)
    .bind(kind_id)
    .bind(user_defined)
    .execute(pool)
    .await
    .expect("seed trigger_instance");
}

async fn kind_id_of(pool: &SqlitePool, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT kind_id FROM trigger_instances WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .expect("fetch kind_id")
}

async fn link_action_to_trigger(pool: &SqlitePool, action_id: &str, trigger_instance_id: &str) {
    sqlx::query("INSERT INTO actions (id, name, queue_id) VALUES (?, 'seed-action', ?)")
        .bind(action_id)
        .bind("00000000000000000000000000")
        .execute(pool)
        .await
        .expect("seed action");
    sqlx::query(
        "INSERT INTO action_trigger_instances (action_id, trigger_instance_id, position) \
         VALUES (?, ?, 0)",
    )
    .bind(action_id)
    .bind(trigger_instance_id)
    .execute(pool)
    .await
    .expect("link action to trigger_instance");
}

async fn link_count_for(pool: &SqlitePool, trigger_instance_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM action_trigger_instances WHERE trigger_instance_id = ?",
    )
    .bind(trigger_instance_id)
    .fetch_one(pool)
    .await
    .expect("count links")
}

#[tokio::test]
async fn default_rows_with_old_ids_are_renamed_in_place() {
    let renames = [
        ("midi.event.note_on", "midi.input.note_on"),
        ("midi.event.note_off", "midi.input.note_off"),
        ("midi.event.control_change", "midi.input.control_change"),
        ("hotkey.triggered", "hotkey.global.pressed"),
    ];

    let pool = fresh_pool().await;
    for (i, (old, _new)) in renames.iter().enumerate() {
        seed(&pool, &format!("def-{i}"), old, 0).await;
    }

    run_migration(&pool).await;

    for (i, (old, new)) in renames.iter().enumerate() {
        assert_eq!(
            kind_id_of(&pool, &format!("def-{i}")).await.as_deref(),
            Some(*new),
            "default row for {old} must be renamed in place to {new}",
        );
    }
}

#[tokio::test]
async fn fk_linked_default_row_survives_migration_and_keeps_its_action_link() {
    let pool = fresh_pool().await;
    seed(&pool, "def-hotkey", "hotkey.triggered", 0).await;
    link_action_to_trigger(&pool, "act-hotkey", "def-hotkey").await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-hotkey").await.as_deref(),
        Some("hotkey.global.pressed"),
        "the FK-referenced default row must be renamed in place, not deleted",
    );
    assert_eq!(
        link_count_for(&pool, "def-hotkey").await,
        1,
        "the action_trigger_instances link must still point at the same row id",
    );
}

#[tokio::test]
async fn user_defined_rows_are_renamed_to_canonical_ids() {
    let renames = [
        ("midi.event.note_on", "midi.input.note_on"),
        ("midi.event.note_off", "midi.input.note_off"),
        ("midi.event.control_change", "midi.input.control_change"),
        ("hotkey.triggered", "hotkey.global.pressed"),
    ];

    let pool = fresh_pool().await;
    for (i, (old, _new)) in renames.iter().enumerate() {
        seed(&pool, &format!("user-{i}"), old, 1).await;
    }

    run_migration(&pool).await;

    for (i, (old, new)) in renames.iter().enumerate() {
        assert_eq!(
            kind_id_of(&pool, &format!("user-{i}")).await.as_deref(),
            Some(*new),
            "user_defined row {old} must be rewritten to {new}",
        );
    }
}

#[tokio::test]
async fn unmapped_kind_ids_are_left_untouched() {
    let pool = fresh_pool().await;
    seed(&pool, "def-unmapped", "obs.scene.current_changed", 0).await;
    seed(&pool, "user-unmapped", "obs.stream.started", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "def-unmapped").await.as_deref(),
        Some("obs.scene.current_changed"),
        "unmapped default row must survive untouched",
    );
    assert_eq!(
        kind_id_of(&pool, "user-unmapped").await.as_deref(),
        Some("obs.stream.started"),
        "unmapped user row must survive untouched",
    );
}

#[tokio::test]
async fn user_defined_old_id_is_renamed_not_deleted() {
    let pool = fresh_pool().await;
    seed(&pool, "user-note-on", "midi.event.note_on", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "user-note-on").await.as_deref(),
        Some("midi.input.note_on"),
        "user row with old id must be renamed, not deleted",
    );
}
