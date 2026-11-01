#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;

use forge_storage_sqlite::{MIGRATIONS, apply_migrations, connect};
use sqlx::SqlitePool;
use tempfile::TempDir;

const VERSION_BEFORE_RENAME: i64 = 72;
const CREATED_AT: i64 = 1_761_328_800;

struct Clip {
    id: &'static str,
    builtin_id: Option<&'static str>,
    stored: &'static str,
    migrated: &'static str,
}

const CLIPS: &[Clip] = &[
    Clip {
        id: "airhorn-builtin",
        builtin_id: Some("airhorn"),
        stored: "Airhorn",
        migrated: "MLG Airhorn",
    },
    Clip {
        id: "airhorn-renamed-by-user",
        builtin_id: Some("airhorn"),
        stored: "My horn",
        migrated: "My horn",
    },
    Clip {
        id: "other-builtin-same-name",
        builtin_id: Some("rimshot"),
        stored: "Airhorn",
        migrated: "Airhorn",
    },
    Clip {
        id: "user-clip-same-name",
        builtin_id: None,
        stored: "Airhorn",
        migrated: "Airhorn",
    },
];

async fn seeded(dir: &TempDir) -> SqlitePool {
    let pool = connect(&common::database_url(dir))
        .await
        .expect("a database");
    MIGRATIONS
        .run_to(VERSION_BEFORE_RENAME, &pool)
        .await
        .expect("the schema before the rename");
    for clip in CLIPS {
        sqlx::query(
            "INSERT INTO soundboard_clips (id, name, file_path, output_device, created_at, builtin_id) VALUES (?, ?, 'clip.wav', 'default', ?, ?)",
        )
        .bind(clip.id)
        .bind(clip.stored)
        .bind(CREATED_AT)
        .bind(clip.builtin_id)
        .execute(&pool)
        .await
        .expect("a seeded clip");
    }
    pool
}

async fn names(pool: &SqlitePool) -> Vec<(String, String)> {
    sqlx::query_as("SELECT id, name FROM soundboard_clips ORDER BY id")
        .fetch_all(pool)
        .await
        .expect("the clips")
}

fn expected_names() -> Vec<(String, String)> {
    let mut expected: Vec<(String, String)> = CLIPS
        .iter()
        .map(|clip| (clip.id.to_owned(), clip.migrated.to_owned()))
        .collect();
    expected.sort();
    expected
}

#[tokio::test]
async fn migration_renames_only_the_untouched_builtin_airhorn_clip() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = seeded(&dir).await;

    apply_migrations(&pool).await.expect("migrate");

    assert_eq!(names(&pool).await, expected_names());
}

#[tokio::test]
async fn a_second_migration_run_leaves_the_clip_names_as_they_were() {
    let dir = TempDir::new().expect("a temporary directory");
    let pool = seeded(&dir).await;
    apply_migrations(&pool).await.expect("first migrate");

    apply_migrations(&pool).await.expect("second migrate");

    assert_eq!(names(&pool).await, expected_names());
}
