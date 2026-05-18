use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::SqliteStorageError;

pub async fn connect(url: &str) -> Result<sqlx::SqlitePool, SqliteStorageError> {
    let opts = SqliteConnectOptions::from_str(url)
        .map_err(SqliteStorageError::Sqlx)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .create_if_missing(true);

    SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await
        .map_err(SqliteStorageError::Sqlx)
}
