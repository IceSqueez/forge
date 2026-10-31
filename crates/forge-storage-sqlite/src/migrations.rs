use sqlx::SqliteConnection;

use crate::error::SqliteStorageError;
use crate::kind_id_rename::{OBS_KIND_ID_RENAME_VERSION, VTUBE_KIND_ID_RENAME_VERSION};

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub const BASELINE_VERSION: i64 = 47;

pub(crate) const JOURNAL_CHECK: &str = "journal check";

const JOURNAL_TABLE: &str = "_sqlx_migrations";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalState {
    Empty,
    Current,
    PreBaseline { found: u32 },
}

pub async fn apply(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
        .run_to(OBS_KIND_ID_RENAME_VERSION - 1, pool)
        .await
        .map_err(migration_failed)?;
    if crate::kind_id_rename::is_pending(pool, OBS_KIND_ID_RENAME_VERSION).await? {
        crate::registry_migration::migrate_registry_format(pool).await?;
        crate::kind_id_rename::apply(pool, OBS_KIND_ID_RENAME_VERSION).await?;
    }
    migrator
        .run_to(VTUBE_KIND_ID_RENAME_VERSION - 1, pool)
        .await
        .map_err(migration_failed)?;
    if crate::kind_id_rename::is_pending(pool, VTUBE_KIND_ID_RENAME_VERSION).await? {
        crate::kind_id_rename::apply(pool, VTUBE_KIND_ID_RENAME_VERSION).await?;
    }
    migrator.run(pool).await.map_err(migration_failed)
}

fn migration_failed(e: sqlx::migrate::MigrateError) -> SqliteStorageError {
    SqliteStorageError::Migration {
        migration: e.to_string(),
        reason: e.to_string(),
    }
}

pub(crate) async fn applied_version(pool: &sqlx::SqlitePool) -> Result<u32, SqliteStorageError> {
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_optional(pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    Ok(version.unwrap_or(0) as u32)
}

pub(crate) async fn prepare_journal(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    let state = {
        let mut conn = pool.acquire().await.map_err(SqliteStorageError::Sqlx)?;
        classify(&mut conn).await?
    };
    match state {
        JournalState::Empty | JournalState::Current => Ok(()),
        JournalState::PreBaseline { found } => Err(SqliteStorageError::PreBaselineSchema { found }),
    }
}

pub async fn classify(conn: &mut SqliteConnection) -> Result<JournalState, SqliteStorageError> {
    let journal_tables: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(JOURNAL_TABLE)
            .fetch_one(&mut *conn)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

    let rows: Vec<(i64, bool)> = if journal_tables == 0 {
        Vec::new()
    } else {
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version")
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

    if rows.iter().any(|(version, _)| *version == BASELINE_VERSION) {
        return Ok(JournalState::Current);
    }

    if let Some((version, _)) = rows.iter().find(|(_, success)| !success) {
        return Err(unrecognized(format!(
            "migration {version} is recorded as failed"
        )));
    }

    let found = rows
        .last()
        .and_then(|(version, _)| u32::try_from(*version).ok())
        .unwrap_or_default();
    Ok(JournalState::PreBaseline { found })
}

fn unrecognized(reason: String) -> SqliteStorageError {
    SqliteStorageError::UnrecognizedJournal { reason }
}
