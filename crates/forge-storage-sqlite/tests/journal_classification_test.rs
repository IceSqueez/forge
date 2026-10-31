#![expect(clippy::expect_used, reason = "test fixtures")]
mod common;
use common::TEST_KEY;

use std::path::PathBuf;

use forge_storage::StorageError;
use forge_storage_sqlite::classify_journal as classify;
use forge_storage_sqlite::{
    BASELINE_VERSION, JournalState, MIGRATIONS, SqliteBackend, SqliteStorageError,
};
use sqlx::SqlitePool;
use tempfile::TempDir;

const PRE_BASELINE_HEAD: i64 = 44;

struct Workspace {
    dir: TempDir,
}

impl Workspace {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("a temporary data dir"),
        }
    }

    fn db(&self) -> PathBuf {
        self.dir.path().join("forge.db")
    }

    fn url(&self) -> String {
        format!("sqlite://{}", self.db().display())
    }

    async fn open(&self) -> Result<SqliteBackend, SqliteStorageError> {
        SqliteBackend::open_for_test(&self.url(), TEST_KEY, self.dir.path().join("media"), None)
            .await
    }

    async fn reconnect(&self) -> SqlitePool {
        forge_storage_sqlite::connect(&self.url())
            .await
            .expect("reconnect")
    }
}

async fn execute(pool: &SqlitePool, sql: &'static str) {
    sqlx::query(sql)
        .execute(pool)
        .await
        .expect("fixture statement");
}

async fn pre_baseline_journal(pool: &SqlitePool) {
    execute(
        pool,
        "CREATE TABLE _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time BIGINT NOT NULL
        )",
    )
    .await;
    for version in 1..=PRE_BASELINE_HEAD {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (?, 'earlier', 1, X'00', 0)",
        )
        .bind(version)
        .execute(pool)
        .await
        .expect("journal row");
    }
    execute(
        pool,
        "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)",
    )
    .await;
}

async fn classify_pool(pool: &SqlitePool) -> Result<JournalState, SqliteStorageError> {
    let mut conn = pool.acquire().await.expect("a connection");
    classify(&mut conn).await
}

#[tokio::test]
async fn classify_reports_empty_for_a_new_file() {
    let workspace = Workspace::new();
    let pool = workspace.reconnect().await;

    assert_eq!(
        classify_pool(&pool).await.expect("classify"),
        JournalState::Empty
    );
}

#[tokio::test]
async fn classify_reports_current_once_the_baseline_is_recorded() {
    let workspace = Workspace::new();
    let pool = workspace.reconnect().await;
    forge_storage_sqlite::apply_migrations(&pool)
        .await
        .expect("baseline");

    assert_eq!(
        classify_pool(&pool).await.expect("classify"),
        JournalState::Current
    );
}

#[tokio::test]
async fn classify_reports_a_journal_without_the_baseline_with_its_last_version() {
    let workspace = Workspace::new();
    let pool = workspace.reconnect().await;
    pre_baseline_journal(&pool).await;

    assert_eq!(
        classify_pool(&pool).await.expect("classify"),
        JournalState::PreBaseline {
            found: PRE_BASELINE_HEAD as u32
        }
    );
}

#[tokio::test]
async fn classify_rejects_a_failed_journal_row() {
    let workspace = Workspace::new();
    let pool = workspace.reconnect().await;
    pre_baseline_journal(&pool).await;
    execute(
        &pool,
        "UPDATE _sqlx_migrations SET success = 0 WHERE version = 30",
    )
    .await;

    let result = classify_pool(&pool).await;

    assert!(matches!(
        result,
        Err(SqliteStorageError::UnrecognizedJournal { .. })
    ));
}

#[tokio::test]
async fn classify_rejects_forge_tables_without_a_journal() {
    let workspace = Workspace::new();
    let pool = workspace.reconnect().await;
    execute(
        &pool,
        "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)",
    )
    .await;

    let result = classify_pool(&pool).await;

    assert!(matches!(
        result,
        Err(SqliteStorageError::UnrecognizedJournal { .. })
    ));
}

#[tokio::test]
async fn opening_a_pre_baseline_database_is_refused_with_the_found_version() {
    let workspace = Workspace::new();
    let pool = workspace.reconnect().await;
    pre_baseline_journal(&pool).await;
    pool.close().await;

    let result = workspace.open().await.err();

    assert!(matches!(
        result,
        Some(SqliteStorageError::PreBaselineSchema { found: 44 })
    ));
}

#[tokio::test]
async fn opening_a_new_database_records_the_baseline_and_every_later_migration() {
    let workspace = Workspace::new();

    drop(workspace.open().await.expect("open a new database"));

    let pool = workspace.reconnect().await;
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("read the journal");
    let expected: Vec<i64> = MIGRATIONS
        .iter()
        .map(|migration| migration.version)
        .filter(|version| *version >= BASELINE_VERSION)
        .collect();
    assert_eq!(versions, expected);
}

#[test]
fn journal_refusals_map_to_the_boot_routes_the_desktop_expects() {
    let pre_baseline = StorageError::from(SqliteStorageError::PreBaselineSchema { found: 44 });
    let unrecognized = StorageError::from(SqliteStorageError::UnrecognizedJournal {
        reason: "gap".to_owned(),
    });

    assert!(matches!(
        pre_baseline,
        StorageError::PreBaselineSchema { found: 44 }
    ));
    assert!(matches!(unrecognized, StorageError::Migration { .. }));
}

#[test]
fn pre_baseline_refusal_names_the_found_version_and_the_release_to_open_it_with() {
    let message = StorageError::PreBaselineSchema { found: 44 }.to_string();

    assert!(
        message.contains("44") && message.contains(forge_storage::LAST_PRE_BASELINE_RELEASE),
        "{message}"
    );
}
