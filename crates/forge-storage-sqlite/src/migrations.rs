use std::io;
use std::path::{Path, PathBuf};

use forge_platform_core::paths::BACKUPS_DIR_NAME;
use rand::rand_core::Rng;
use sqlx::SqliteConnection;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;
use crate::legacy_chain::LEGACY_CHAIN;

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub const BASELINE_VERSION: i64 = 47;

pub(crate) const JOURNAL_CHECK: &str = "journal check";

const JOURNAL_TABLE: &str = "_sqlx_migrations";
const SKIPPED_EXECUTION_TIME: i64 = -1;
const SNAPSHOT_PREFIX: &str = "forge-pre-baseline";
const SNAPSHOT_EXTENSION: &str = "db";
const SNAPSHOT_SUFFIX_BYTES: usize = 4;

#[cfg(unix)]
const OWNER_ONLY_MODE: u32 = 0o600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalState {
    Empty,
    Current,
    LegacyComplete,
    LegacyPartial { found: u32 },
}

pub async fn apply(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
        .run(pool)
        .await
        .map_err(|e| SqliteStorageError::Migration {
            migration: e.to_string(),
            reason: e.to_string(),
        })
}

pub async fn applied_version(pool: &sqlx::SqlitePool) -> Result<u32, SqliteStorageError> {
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_optional(pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    Ok(version.unwrap_or(0) as u32)
}

pub async fn prepare_journal(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    let state = {
        let mut conn = pool.acquire().await.map_err(SqliteStorageError::Sqlx)?;
        classify(&mut conn).await?
    };
    match state {
        JournalState::Empty | JournalState::Current => Ok(()),
        JournalState::LegacyPartial { found } => {
            Err(SqliteStorageError::PreBaselineSchema { found })
        }
        JournalState::LegacyComplete => adopt(pool).await,
    }
}

pub async fn classify(conn: &mut SqliteConnection) -> Result<JournalState, SqliteStorageError> {
    let journal_tables: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(JOURNAL_TABLE)
            .fetch_one(&mut *conn)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

    let rows: Vec<(i64, bool, Vec<u8>)> = if journal_tables == 0 {
        Vec::new()
    } else {
        sqlx::query_as("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut *conn)
            .await
            .map_err(SqliteStorageError::Sqlx)?
    };

    if rows.is_empty() {
        let schema_tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite!_%' ESCAPE '!' AND name <> ?",
        )
        .bind(JOURNAL_TABLE)
        .fetch_one(&mut *conn)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        return if schema_tables == 0 {
            Ok(JournalState::Empty)
        } else {
            Err(unrecognized(
                "tables exist without an applied migration journal".to_owned(),
            ))
        };
    }

    if rows
        .iter()
        .any(|(version, _, _)| *version == BASELINE_VERSION)
    {
        return Ok(JournalState::Current);
    }

    if let Some((version, _, _)) = rows.iter().find(|(_, success, _)| !success) {
        return Err(unrecognized(format!(
            "migration {version} is recorded as failed"
        )));
    }

    for (index, (version, _, checksum)) in rows.iter().enumerate() {
        let Some(frozen) = LEGACY_CHAIN.get(index) else {
            return Err(unrecognized(format!(
                "migration {version} is not part of the legacy chain"
            )));
        };
        if frozen.version != *version {
            return Err(unrecognized(format!(
                "migration {version} is recorded where the legacy chain expects {}",
                frozen.version
            )));
        }
        if !frozen.matches(&to_hex(checksum)) {
            return Err(unrecognized(format!(
                "checksum of migration {version} differs from the shipped migration"
            )));
        }
    }

    if rows.len() == LEGACY_CHAIN.len() {
        return Ok(JournalState::LegacyComplete);
    }
    let found = rows
        .last()
        .and_then(|(version, _, _)| u32::try_from(*version).ok())
        .unwrap_or_default();
    Ok(JournalState::LegacyPartial { found })
}

async fn adopt(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    let snapshot = write_snapshot(pool).await?;

    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(SqliteStorageError::Sqlx)?;
    let adopted = match classify(&mut tx).await? {
        JournalState::LegacyComplete => {
            insert_baseline_row(&mut tx).await?;
            true
        }
        JournalState::Current => false,
        JournalState::LegacyPartial { found } => {
            return Err(SqliteStorageError::PreBaselineSchema { found });
        }
        JournalState::Empty => {
            return Err(unrecognized(
                "the migration journal emptied during adoption".to_owned(),
            ));
        }
    };
    tx.commit().await.map_err(SqliteStorageError::Sqlx)?;

    if adopted {
        let snapshot_name = snapshot
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        tracing::info!(
            snapshot = %snapshot_name,
            baseline = BASELINE_VERSION,
            "adopted the legacy migration journal into the baseline schema"
        );
    }
    Ok(())
}

async fn insert_baseline_row(conn: &mut SqliteConnection) -> Result<(), SqliteStorageError> {
    let baseline = MIGRATIONS
        .iter()
        .find(|migration| migration.version == BASELINE_VERSION)
        .ok_or_else(|| unrecognized("the embedded migrations lack the baseline".to_owned()))?;

    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(baseline.version)
    .bind(baseline.description.as_ref())
    .bind(true)
    .bind(baseline.checksum.as_ref())
    .bind(SKIPPED_EXECUTION_TIME)
    .execute(&mut *conn)
    .await
    .map_err(SqliteStorageError::Sqlx)?;
    Ok(())
}

async fn write_snapshot(pool: &sqlx::SqlitePool) -> Result<PathBuf, SqliteStorageError> {
    let db_file: String =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .fetch_one(pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
    let dir = Path::new(&db_file)
        .parent()
        .filter(|_| !db_file.is_empty())
        .ok_or_else(|| snapshot_failed("the database has no file to snapshot".to_owned()))?
        .join(BACKUPS_DIR_NAME);

    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| snapshot_failed(format!("could not create the backups directory: {e}")))?;

    let path = dir.join(snapshot_file_name(OffsetDateTime::now_utc()));
    let target = path
        .to_str()
        .ok_or_else(|| snapshot_failed("the backups path is not valid UTF-8".to_owned()))?
        .to_owned();
    create_private(&path)
        .await
        .map_err(|e| snapshot_failed(format!("could not create the snapshot file: {e}")))?;

    let written = match sqlx::query("VACUUM INTO ?")
        .bind(target)
        .execute(pool)
        .await
    {
        Ok(_) => sync_file(&path)
            .await
            .map_err(|e| format!("could not flush the snapshot file: {e}")),
        Err(e) => Err(format!("could not copy the database: {e}")),
    };

    match written {
        Ok(()) => Ok(path),
        Err(reason) => {
            if let Err(e) = tokio::fs::remove_file(&path).await {
                tracing::warn!(error = %e, "could not remove the incomplete pre-baseline snapshot");
            }
            Err(snapshot_failed(reason))
        }
    }
}

fn snapshot_file_name(now: OffsetDateTime) -> String {
    let mut suffix = [0u8; SNAPSHOT_SUFFIX_BYTES];
    rand::rng().fill_bytes(&mut suffix);
    format!(
        "{SNAPSHOT_PREFIX}-{:04}{:02}{:02}-{:02}{:02}{:02}Z-{}.{SNAPSHOT_EXTENSION}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        to_hex(&suffix),
    )
}

async fn create_private(path: &Path) -> io::Result<()> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(OWNER_ONLY_MODE);
    options.open(path).await.map(drop)
}

async fn sync_file(path: &Path) -> io::Result<()> {
    tokio::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .await?
        .sync_all()
        .await
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

fn unrecognized(reason: String) -> SqliteStorageError {
    SqliteStorageError::UnrecognizedJournal { reason }
}

fn snapshot_failed(reason: String) -> SqliteStorageError {
    SqliteStorageError::AdoptionSnapshot { reason }
}
