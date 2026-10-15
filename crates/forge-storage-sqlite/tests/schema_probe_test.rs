#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use forge_storage::{DataProvider, EXPECTED_SCHEMA_VERSION};
use forge_storage_sqlite::{
    SqliteBackend, SqliteStorageError, apply_migrations, connect_pools, read_applied_schema_version,
};
use tempfile::TempDir;

const TEST_KEY: [u8; 32] = [0x5a; 32];

fn url_of(db: &Path) -> String {
    format!("sqlite://{}", db.display())
}

fn db_in(dir: &TempDir) -> PathBuf {
    dir.path().join("forge.db")
}

fn entries(dir: &TempDir) -> BTreeSet<String> {
    std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn migrate_and_shut_down(dir: &TempDir) {
    let db = db_in(dir);
    let media = dir.path().join("media");
    runtime().block_on(async {
        let backend = SqliteBackend::open_for_test(&url_of(&db), TEST_KEY, media, None)
            .await
            .unwrap();
        backend.shutdown().await;
    });
}

#[test]
fn missing_file_yields_none_and_is_not_created() {
    let dir = tempfile::tempdir().unwrap();
    let db = db_in(&dir);

    assert!(matches!(read_applied_schema_version(&db), Ok(None)));
    assert!(entries(&dir).is_empty());
}

#[test]
fn database_without_journal_table_yields_none() {
    let dir = tempfile::tempdir().unwrap();
    let db = db_in(&dir);
    runtime().block_on(async {
        let pools = connect_pools(&url_of(&db)).await.unwrap();
        sqlx::query("CREATE TABLE unrelated (id INTEGER PRIMARY KEY)")
            .execute(pools.writer())
            .await
            .unwrap();
        pools.close().await;
    });

    assert!(matches!(read_applied_schema_version(&db), Ok(None)));
}

#[test]
fn freshly_migrated_backend_reports_expected_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    migrate_and_shut_down(&dir);

    let found = read_applied_schema_version(&db_in(&dir));

    assert!(matches!(found, Ok(Some(v)) if v == EXPECTED_SCHEMA_VERSION));
}

#[test]
fn version_committed_only_in_wal_is_visible() {
    let dir = tempfile::tempdir().unwrap();
    let db = db_in(&dir);
    let rt = runtime();
    let pools = rt.block_on(async {
        let pools = connect_pools(&url_of(&db)).await.unwrap();
        apply_migrations(pools.writer()).await.unwrap();
        pools
    });
    let wal_len = std::fs::metadata(db.with_extension("db-wal"))
        .unwrap()
        .len();
    assert!(wal_len > 0);

    let found = read_applied_schema_version(&db);

    assert!(matches!(found, Ok(Some(v)) if v == EXPECTED_SCHEMA_VERSION));
    rt.block_on(pools.close());
}

#[test]
fn probe_leaves_main_file_bytes_and_mtime_untouched() {
    let dir = tempfile::tempdir().unwrap();
    migrate_and_shut_down(&dir);
    let db = db_in(&dir);
    let bytes_before = std::fs::read(&db).unwrap();
    let mtime_before = std::fs::metadata(&db).unwrap().modified().unwrap();

    assert!(read_applied_schema_version(&db).is_ok());

    assert_eq!(std::fs::read(&db).unwrap(), bytes_before);
    assert_eq!(
        std::fs::metadata(&db).unwrap().modified().unwrap(),
        mtime_before
    );
}

#[test]
fn garbage_file_yields_error_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let db = db_in(&dir);
    std::fs::write(
        &db,
        b"this is definitely not a sqlite database file, ".repeat(64),
    )
    .unwrap();

    let found = read_applied_schema_version(&db);

    assert!(matches!(found, Err(SqliteStorageError::Sqlx(_))));
}
