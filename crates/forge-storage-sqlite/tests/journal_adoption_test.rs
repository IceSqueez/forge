#![allow(clippy::expect_used)]

mod legacy_fixture;

use std::path::PathBuf;

use forge_storage::StorageError;
use forge_storage_sqlite::migrations::classify;
use forge_storage_sqlite::{
    BASELINE_VERSION, JournalState, MIGRATIONS, SqliteBackend, SqliteStorageError,
};
use legacy_fixture::{
    LEGACY_HEAD, data_fingerprint, db_url, journal, legacy_db, legacy_migrator,
    rewrite_journal_to_crlf,
};
use sqlx::SqlitePool;
use tempfile::TempDir;

const TEST_KEY: [u8; 32] = [0xab; 32];
const SNAPSHOT_PREFIX: &str = "forge-pre-baseline-";

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
        db_url(&self.db())
    }

    fn backups(&self) -> PathBuf {
        self.dir
            .path()
            .join(forge_platform_core::paths::BACKUPS_DIR_NAME)
    }

    fn snapshots(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(self.backups()) else {
            return Vec::new();
        };
        let mut found: Vec<PathBuf> = entries
            .map(|entry| entry.expect("a backups entry").path())
            .filter(|path| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(SNAPSHOT_PREFIX) && n.ends_with(".db"))
            })
            .collect();
        found.sort();
        found
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

async fn legacy_workspace(up_to: i64) -> Workspace {
    let workspace = Workspace::new();
    let pool = legacy_db(&workspace.db(), up_to).await;
    sqlx::query("INSERT INTO settings (key, value) VALUES ('user.marker', 'kept')")
        .execute(&pool)
        .await
        .expect("seed user data");
    pool.close().await;
    workspace
}

async fn classify_pool(pool: &SqlitePool) -> Result<JournalState, SqliteStorageError> {
    let mut conn = pool.acquire().await.expect("a connection");
    classify(&mut conn).await
}

async fn execute(pool: &SqlitePool, sql: &'static str) {
    sqlx::query(sql)
        .execute(pool)
        .await
        .expect("fixture statement");
}

#[tokio::test]
async fn classify_reports_empty_for_a_new_file_and_for_an_empty_journal() {
    let workspace = Workspace::new();
    let pool = workspace.reconnect().await;
    let without_journal = classify_pool(&pool).await.expect("classify");
    legacy_migrator()
        .await
        .run_to(0, &pool)
        .await
        .expect("create an empty journal");

    let with_empty_journal = classify_pool(&pool).await.expect("classify");

    assert_eq!(
        (without_journal, with_empty_journal),
        (JournalState::Empty, JournalState::Empty)
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
async fn classify_reports_legacy_complete_for_a_full_lf_journal() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    let pool = workspace.reconnect().await;

    assert_eq!(
        classify_pool(&pool).await.expect("classify"),
        JournalState::LegacyComplete
    );
}

#[tokio::test]
async fn classify_reports_legacy_complete_for_a_full_crlf_journal() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    let pool = workspace.reconnect().await;
    rewrite_journal_to_crlf(&pool).await;

    assert_eq!(
        classify_pool(&pool).await.expect("classify"),
        JournalState::LegacyComplete
    );
}

#[tokio::test]
async fn classify_reports_legacy_partial_with_the_last_applied_version() {
    for found in [1_i64, 44, LEGACY_HEAD - 1] {
        let workspace = legacy_workspace(found).await;
        let pool = workspace.reconnect().await;

        let state = classify_pool(&pool).await.expect("classify");

        assert_eq!(
            state,
            JournalState::LegacyPartial {
                found: found as u32
            },
            "prefix 1..={found}"
        );
    }
}

#[tokio::test]
async fn classify_rejects_journals_that_are_not_a_shipped_prefix() {
    let corruptions: [(&str, &'static str); 4] = [
        (
            "failed row",
            "UPDATE _sqlx_migrations SET success = 0 WHERE version = 30",
        ),
        ("gap", "DELETE FROM _sqlx_migrations WHERE version = 20"),
        (
            "foreign checksum",
            "UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 10",
        ),
        (
            "unknown version",
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (99, 'stray', 1, X'00', 0)",
        ),
    ];
    for (case, corruption) in corruptions {
        let workspace = legacy_workspace(LEGACY_HEAD).await;
        let pool = workspace.reconnect().await;
        execute(&pool, corruption).await;

        let result = classify_pool(&pool).await;

        assert!(
            matches!(result, Err(SqliteStorageError::UnrecognizedJournal { .. })),
            "{case}: {result:?}"
        );
    }
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
async fn opening_a_partial_legacy_database_is_refused_with_the_found_version() {
    let workspace = legacy_workspace(44).await;

    let result = workspace.open().await.err();

    assert!(matches!(
        result,
        Some(SqliteStorageError::PreBaselineSchema { found: 44 })
    ));
}

#[tokio::test]
async fn a_refused_partial_legacy_database_is_left_unwritten() {
    let workspace = legacy_workspace(44).await;
    let pool = workspace.reconnect().await;
    let journal_before = journal(&pool).await;
    let data_before = data_fingerprint(&pool).await;
    pool.close().await;

    assert!(workspace.open().await.is_err());

    let pool = workspace.reconnect().await;
    assert_eq!(
        (
            journal(&pool).await,
            data_fingerprint(&pool).await,
            workspace.backups().exists()
        ),
        (journal_before, data_before, false)
    );
}

#[tokio::test]
async fn a_refused_unrecognized_database_is_left_unwritten() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    let pool = workspace.reconnect().await;
    execute(
        &pool,
        "UPDATE _sqlx_migrations SET success = 0 WHERE version = 46",
    )
    .await;
    let journal_before = journal(&pool).await;
    pool.close().await;

    let result = workspace.open().await.err();

    let pool = workspace.reconnect().await;
    assert!(matches!(
        result,
        Some(SqliteStorageError::UnrecognizedJournal { .. })
    ));
    assert_eq!(
        (journal(&pool).await, workspace.backups().exists()),
        (journal_before, false)
    );
}

#[tokio::test]
async fn opening_a_new_database_records_only_the_baseline_and_takes_no_snapshot() {
    let workspace = Workspace::new();

    drop(workspace.open().await.expect("open a new database"));

    let pool = workspace.reconnect().await;
    let versions: Vec<i64> = journal(&pool).await.into_iter().map(|row| row.0).collect();
    assert_eq!(
        (versions, workspace.backups().exists()),
        (vec![BASELINE_VERSION], false)
    );
}

#[tokio::test]
async fn adopting_a_legacy_database_appends_a_baseline_row_and_keeps_rows_one_to_forty_six() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    let pool = workspace.reconnect().await;
    let legacy_rows = journal(&pool).await;
    pool.close().await;

    drop(workspace.open().await.expect("adoption"));

    let pool = workspace.reconnect().await;
    let mut rows = journal(&pool).await;
    let baseline = rows.pop().expect("a baseline row");
    let embedded = MIGRATIONS
        .iter()
        .find(|m| m.version == BASELINE_VERSION)
        .expect("the embedded baseline");
    assert_eq!(rows, legacy_rows);
    assert_eq!(
        (baseline.0, baseline.1, baseline.3, baseline.4, baseline.5),
        (
            BASELINE_VERSION,
            embedded.description.to_string(),
            true,
            embedded.checksum.to_vec(),
            -1
        )
    );
}

#[tokio::test]
async fn adopting_a_crlf_legacy_database_succeeds() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    let pool = workspace.reconnect().await;
    rewrite_journal_to_crlf(&pool).await;
    pool.close().await;

    drop(workspace.open().await.expect("adoption"));

    let pool = workspace.reconnect().await;
    assert_eq!(
        classify_pool(&pool).await.expect("classify"),
        JournalState::Current
    );
}

#[tokio::test]
async fn adoption_leaves_the_existing_data_untouched() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    let pool = workspace.reconnect().await;
    let before = data_fingerprint(&pool).await;
    pool.close().await;

    drop(workspace.open().await.expect("adoption"));

    let pool = workspace.reconnect().await;
    assert_eq!(data_fingerprint(&pool).await, before);
}

#[tokio::test]
async fn adoption_writes_one_snapshot_holding_the_pre_adoption_data_and_journal() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    let pool = workspace.reconnect().await;
    let data_before = data_fingerprint(&pool).await;
    let journal_before = journal(&pool).await;
    pool.close().await;

    drop(workspace.open().await.expect("adoption"));

    let snapshots = workspace.snapshots();
    assert_eq!(snapshots.len(), 1);
    let snapshot = forge_storage_sqlite::connect(&db_url(&snapshots[0]))
        .await
        .expect("open the snapshot");
    assert_eq!(
        (data_fingerprint(&snapshot).await, journal(&snapshot).await),
        (data_before, journal_before)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn adoption_snapshot_is_readable_by_the_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = legacy_workspace(LEGACY_HEAD).await;

    drop(workspace.open().await.expect("adoption"));

    let snapshot = workspace.snapshots().pop().expect("a snapshot");
    let mode = std::fs::metadata(snapshot)
        .expect("snapshot metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn a_failed_snapshot_aborts_adoption_and_leaves_the_journal_legacy_complete() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    std::fs::write(workspace.backups(), b"not a directory").expect("block the backups dir");

    let result = workspace.open().await.err();

    let pool = workspace.reconnect().await;
    assert!(matches!(
        result,
        Some(SqliteStorageError::AdoptionSnapshot { .. })
    ));
    assert_eq!(
        classify_pool(&pool).await.expect("classify"),
        JournalState::LegacyComplete
    );
}

#[tokio::test]
async fn reopening_an_adopted_database_takes_no_new_snapshot_and_keeps_the_journal() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    drop(workspace.open().await.expect("adoption"));
    let pool = workspace.reconnect().await;
    let journal_after_adoption = journal(&pool).await;
    pool.close().await;

    drop(workspace.open().await.expect("reopen"));

    let pool = workspace.reconnect().await;
    assert_eq!(
        (journal(&pool).await, workspace.snapshots().len()),
        (journal_after_adoption, 1)
    );
}

#[tokio::test]
async fn an_adoption_interrupted_before_commit_is_repeated_on_the_next_open() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    let pool = workspace.reconnect().await;
    {
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("an immediate transaction");
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
             VALUES (47, 'baseline', 1, X'00', -1)",
        )
        .execute(&mut *tx)
        .await
        .expect("an uncommitted baseline row");
    }
    pool.close().await;
    std::fs::create_dir_all(workspace.backups()).expect("backups dir");
    let leftover = workspace
        .backups()
        .join(format!("{SNAPSHOT_PREFIX}20261003-000000Z-00000000.db"));
    std::fs::write(&leftover, b"interrupted snapshot").expect("a leftover snapshot");

    drop(
        workspace
            .open()
            .await
            .expect("adoption after the interruption"),
    );

    let pool = workspace.reconnect().await;
    assert_eq!(
        classify_pool(&pool).await.expect("classify"),
        JournalState::Current
    );
    assert_eq!(
        (
            workspace.snapshots().len(),
            std::fs::read(&leftover).expect("the leftover survives")
        ),
        (2, b"interrupted snapshot".to_vec())
    );
}

#[tokio::test]
async fn a_pre_baseline_build_does_not_replay_its_chain_on_an_adopted_database() {
    let workspace = legacy_workspace(LEGACY_HEAD).await;
    drop(workspace.open().await.expect("adoption"));
    let pool = workspace.reconnect().await;
    let journal_after_adoption = journal(&pool).await;
    let data_after_adoption = data_fingerprint(&pool).await;
    let mut old_build = legacy_migrator().await;
    old_build.set_ignore_missing(true);

    old_build
        .run(&pool)
        .await
        .expect("the old migrator sees nothing to apply");

    assert_eq!(
        (journal(&pool).await, data_fingerprint(&pool).await),
        (journal_after_adoption, data_after_adoption)
    );
}

#[test]
fn journal_refusals_map_to_the_boot_routes_the_desktop_expects() {
    let pre_baseline = StorageError::from(SqliteStorageError::PreBaselineSchema { found: 44 });
    let unrecognized = StorageError::from(SqliteStorageError::UnrecognizedJournal {
        reason: "gap".to_owned(),
    });
    let snapshot = StorageError::from(SqliteStorageError::AdoptionSnapshot {
        reason: "disk full".to_owned(),
    });

    assert!(matches!(
        pre_baseline,
        StorageError::PreBaselineSchema { found: 44 }
    ));
    assert!(matches!(unrecognized, StorageError::Migration { .. }));
    assert!(matches!(snapshot, StorageError::Migration { .. }));
}

#[test]
fn pre_baseline_refusal_names_the_found_version_and_the_release_to_open_it_with() {
    let message = StorageError::PreBaselineSchema { found: 44 }.to_string();

    assert!(
        message.contains("44") && message.contains(forge_storage::LAST_PRE_BASELINE_RELEASE),
        "{message}"
    );
}
