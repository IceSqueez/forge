use crate::error::SqliteStorageError;

pub static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn apply(pool: &sqlx::SqlitePool) -> Result<(), SqliteStorageError> {
    MIGRATIONS
        .run(pool)
        .await
        .map_err(|e| SqliteStorageError::Migration {
            migration: e.to_string(),
            reason: e.to_string(),
        })
}
