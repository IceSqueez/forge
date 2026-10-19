#![allow(clippy::unwrap_used, clippy::expect_used)]

mod legacy_fixture;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use forge_storage::{
    CredentialId, CredentialsRepo, DataProvider, DatabaseFileSchema, EXPECTED_SCHEMA_VERSION,
    StoredDatabase, StoredSchema,
};
use forge_storage_sqlite::{
    SqliteBackend, SqliteCredentialsRepo, SqliteDatabaseFileSchema, SqliteStorageError,
    apply_migrations, connect_pools, inspect_database_file, read_applied_schema_version,
};
use legacy_fixture::{LEGACY_HEAD, legacy_db};
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
fn database_file_schema_port_reports_the_migrated_version() {
    let dir = tempfile::tempdir().unwrap();
    migrate_and_shut_down(&dir);
    let probe: &dyn DatabaseFileSchema = &SqliteDatabaseFileSchema;

    let found = probe.applied_schema_version(&db_in(&dir));

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

fn inspected(db: &Path) -> StoredDatabase {
    inspect_database_file(db).unwrap().unwrap()
}

#[test]
fn inspect_reports_a_migrated_database_as_current_without_credentials() {
    let dir = tempfile::tempdir().unwrap();
    migrate_and_shut_down(&dir);

    assert_eq!(
        inspected(&db_in(&dir)),
        StoredDatabase {
            schema: StoredSchema::Current {
                version: EXPECTED_SCHEMA_VERSION
            },
            holds_credentials: false,
        }
    );
}

#[test]
fn inspect_reports_holding_credentials_once_one_is_stored() {
    let dir = tempfile::tempdir().unwrap();
    migrate_and_shut_down(&dir);
    runtime().block_on(async {
        let pool = forge_storage_sqlite::connect(&url_of(&db_in(&dir)))
            .await
            .unwrap();
        SqliteCredentialsRepo::new_with_key(pool.clone(), TEST_KEY)
            .store(&CredentialId::new("twitch:broadcaster"), "token")
            .await
            .unwrap();
        pool.close().await;
    });

    assert!(inspected(&db_in(&dir)).holds_credentials);
}

#[test]
fn inspect_classifies_legacy_journals_by_completeness() {
    for (up_to, expected) in [
        (
            LEGACY_HEAD - 2,
            StoredSchema::PreBaseline {
                found: u32::try_from(LEGACY_HEAD - 2).unwrap(),
            },
        ),
        (
            LEGACY_HEAD,
            StoredSchema::AdoptableLegacy {
                version: u32::try_from(LEGACY_HEAD).unwrap(),
            },
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = db_in(&dir);
        runtime().block_on(async { legacy_db(&db, up_to).await.close().await });

        assert_eq!(
            inspected(&db).schema,
            expected,
            "legacy chain up to {up_to}"
        );
    }
}

#[test]
fn inspect_reports_tables_without_a_journal_as_unrecognized() {
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

    assert_eq!(inspected(&db).schema, StoredSchema::Unrecognized);
}

#[test]
fn inspect_of_a_missing_file_is_none_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();

    assert!(matches!(inspect_database_file(&db_in(&dir)), Ok(None)));
    assert!(entries(&dir).is_empty());
}
