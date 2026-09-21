#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use forge_storage_sqlite::{MIGRATIONS, connect};
use sqlx::SqlitePool;

const MEDIA_MIGRATION_SQL: &str = include_str!("../migrations/0044_media_blobs.sql");
const SCHEMA_VERSION_BEFORE_MEDIA: i64 = 43;
const CLIP_ID: &str = "clip-before-media";
const CLIP_PATH: &str = "/home/streamer/sounds/fanfare.wav";

async fn pool_at_version(version: i64) -> SqlitePool {
    let pool = connect("sqlite::memory:").await.expect("connect");
    for migration in MIGRATIONS.iter().filter(|m| m.version <= version) {
        sqlx::raw_sql(migration.sql.clone())
            .execute(&pool)
            .await
            .unwrap_or_else(|error| panic!("migration {} failed: {error}", migration.version));
    }
    pool
}

async fn seed_clip(pool: &SqlitePool) {
    sqlx::query(
        "INSERT INTO soundboard_clips (id, name, file_path, volume, output_device, created_at) \
         VALUES (?, 'Fanfare', ?, 1.0, '\"default\"', 0)",
    )
    .bind(CLIP_ID)
    .bind(CLIP_PATH)
    .execute(pool)
    .await
    .expect("seed soundboard clip");
}

async fn table_names(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .fetch_all(pool)
        .await
        .expect("query sqlite_master")
}

async fn apply_media_migration(pool: &SqlitePool) {
    sqlx::raw_sql(MEDIA_MIGRATION_SQL)
        .execute(pool)
        .await
        .expect("apply the media blobs migration");
}

#[tokio::test]
async fn the_media_migration_adds_its_tables_to_a_database_that_predates_them() {
    let pool = pool_at_version(SCHEMA_VERSION_BEFORE_MEDIA).await;
    let before = table_names(&pool).await;
    assert!(
        !before.iter().any(|name| name == "media_blobs"),
        "the pre-media schema already carried media_blobs"
    );

    apply_media_migration(&pool).await;

    let after = table_names(&pool).await;
    for table in ["media_blobs", "media_references"] {
        assert!(
            after.iter().any(|name| name == table),
            "missing table after the media migration: {table}"
        );
    }
}

#[tokio::test]
async fn the_media_migration_leaves_the_existing_soundboard_rows_untouched() {
    let pool = pool_at_version(SCHEMA_VERSION_BEFORE_MEDIA).await;
    seed_clip(&pool).await;

    apply_media_migration(&pool).await;

    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id, file_path FROM soundboard_clips ORDER BY id")
            .fetch_all(&pool)
            .await
            .expect("read soundboard clips");

    assert_eq!(rows, vec![(CLIP_ID.to_owned(), CLIP_PATH.to_owned())]);
}

#[tokio::test]
async fn the_media_migration_adds_no_column_to_the_soundboard_table() {
    let pool = pool_at_version(SCHEMA_VERSION_BEFORE_MEDIA).await;
    let before: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('soundboard_clips') ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("read soundboard columns");

    apply_media_migration(&pool).await;

    let after: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('soundboard_clips') ORDER BY name")
            .fetch_all(&pool)
            .await
            .expect("read soundboard columns");

    assert_eq!(after, before);
}

#[tokio::test]
async fn the_media_migration_is_safe_to_replay() {
    let pool = pool_at_version(SCHEMA_VERSION_BEFORE_MEDIA).await;
    apply_media_migration(&pool).await;

    sqlx::query("INSERT INTO media_blobs (id, format, byte_size, label, imported_at) VALUES ('sha256-x', 'gif', 6, 'a.gif', 0)")
        .execute(&pool)
        .await
        .expect("seed a blob row");

    apply_media_migration(&pool).await;

    let blobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_blobs")
        .fetch_one(&pool)
        .await
        .expect("count blobs");
    assert_eq!(blobs, 1);
}
