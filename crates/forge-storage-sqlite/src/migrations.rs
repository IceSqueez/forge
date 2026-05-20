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
