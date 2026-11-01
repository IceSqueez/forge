use sqlx::SqliteConnection;

use crate::codec::row_int::decode_u32;
use crate::error::SqliteStorageError;
use crate::schema::kind_id_rename::{OBS_KIND_ID_RENAME_VERSION, VTUBE_KIND_ID_RENAME_VERSION};

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub const BASELINE_VERSION: i64 = 47;

pub(crate) const JOURNAL_CHECK: &str = "journal check";

const JOURNAL_TABLE: &str = "_sqlx_migrations";
const UNIDENTIFIED_MIGRATION: &str = "unidentified";

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
    if crate::schema::kind_id_rename::is_pending(pool, OBS_KIND_ID_RENAME_VERSION).await? {
        crate::schema::registry_migration::migrate_registry_format(pool).await?;
        crate::schema::kind_id_rename::apply(pool, OBS_KIND_ID_RENAME_VERSION).await?;
    }
    migrator
        .run_to(VTUBE_KIND_ID_RENAME_VERSION - 1, pool)
        .await
        .map_err(migration_failed)?;
    if crate::schema::kind_id_rename::is_pending(pool, VTUBE_KIND_ID_RENAME_VERSION).await? {
        crate::schema::kind_id_rename::apply(pool, VTUBE_KIND_ID_RENAME_VERSION).await?;
    }
    migrator.run(pool).await.map_err(migration_failed)
}

fn migration_failed(e: sqlx::migrate::MigrateError) -> SqliteStorageError {
    use sqlx::migrate::MigrateError;

    let migration = match &e {
        MigrateError::ExecuteMigration(_, version)
        | MigrateError::VersionMissing(version)
        | MigrateError::VersionMismatch(version)
        | MigrateError::VersionNotPresent(version)
        | MigrateError::Dirty(version) => version.to_string(),
        _ => UNIDENTIFIED_MIGRATION.to_owned(),
    };
    SqliteStorageError::Migration {
        migration,
        reason: e.to_string(),
    }
}

pub(crate) async fn applied_version(pool: &sqlx::SqlitePool) -> Result<u32, SqliteStorageError> {
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_optional(pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    decode_u32("migration version", version.unwrap_or(0))
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

#[cfg(test)]
mod tests {
    use sqlx::migrate::MigrateError;

    use super::*;

    #[test]
    fn a_failed_migration_names_the_version_the_error_carries() {
        for (error, expected) in [
            (
                MigrateError::ExecuteMigration(sqlx::Error::PoolClosed, 12),
                "12",
            ),
            (MigrateError::VersionMissing(13), "13"),
            (MigrateError::VersionMismatch(14), "14"),
            (MigrateError::VersionNotPresent(15), "15"),
            (MigrateError::Dirty(16), "16"),
            (MigrateError::VersionTooOld(1, 2), UNIDENTIFIED_MIGRATION),
        ] {
            let SqliteStorageError::Migration { migration, .. } = migration_failed(error) else {
                panic!("a migrate error must stay a migration failure");
            };
            assert_eq!(migration, expected);
        }
    }
}
