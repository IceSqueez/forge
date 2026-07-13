use crate::error::SqliteStorageError;

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

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

/// Highest migration version recorded in `_sqlx_migrations`; `0` on a database that
/// has never had a migration applied. Callers must invoke this only after [`apply`].
pub async fn applied_version(pool: &sqlx::SqlitePool) -> Result<u32, SqliteStorageError> {
    let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_optional(pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    Ok(version.unwrap_or(0) as u32)
}
