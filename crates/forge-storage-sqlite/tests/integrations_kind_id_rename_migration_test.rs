#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Regression coverage for the MIDI / Hotkey `kind_id` rewrite data migration
//! (`0020_integrations_kind_id_rename.sql`).
//!
//! `apply_migrations` runs every migration (incl. 0020) up front, so pre-0020
//! rows cannot be inserted through the normal boot path. Mirroring the
//! 0018/0019 pattern, each test applies the full schema, seeds rows carrying
//! the OLD `kind_id` literals (no constraint forbids them post-migration), then
//! runs the REAL migration SQL loaded verbatim from the file so the production
//! statements — not a reimplementation — are under test.

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

#[tokio::test]
async fn default_rows_with_old_ids_are_renamed_in_place() {
    // Default rows must be UPDATEd in place (not deleted): action_trigger_instances
    // has an ON DELETE RESTRICT FK to trigger_instances, so deleting a default that
    // an action is attached to would abort the whole migration. Boot's
    // upsert_default later normalizes the surviving default's name/config.
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
async fn user_defined_rows_are_renamed_to_canonical_ids() {
    // All four straight 1-to-1 renames; this migration has no consolidation.
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
    // A default row and a user row, neither in the rewrite set.
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
    // Distinguishes the two arms: a user row carrying an old id must be UPDATEd,
    // never deleted by the default-row DELETE.
    let pool = fresh_pool().await;
    seed(&pool, "user-note-on", "midi.event.note_on", 1).await;

    run_migration(&pool).await;

    assert_eq!(
        kind_id_of(&pool, "user-note-on").await.as_deref(),
        Some("midi.input.note_on"),
        "user row with old id must be renamed, not deleted",
    );
}
