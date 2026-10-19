use std::io;
use std::path::Path;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Connection};

use crate::error::SqliteStorageError;

const JOURNAL_TABLE_EXISTS: &str =
    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')";
const APPLIED_VERSION: &str = "SELECT MAX(version) FROM _sqlx_migrations";

pub fn read_applied_schema_version(path: &Path) -> Result<Option<u32>, SqliteStorageError> {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Ok(None),
        Err(e) => return Err(SqliteStorageError::Sqlx(sqlx::Error::Io(e))),
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e: io::Error| SqliteStorageError::Sqlx(sqlx::Error::Io(e)))?;
    runtime.block_on(read_version(path))
}

async fn read_version(path: &Path) -> Result<Option<u32>, SqliteStorageError> {
    let mut conn = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .connect()
        .await?;
    let has_journal: bool = sqlx::query_scalar(JOURNAL_TABLE_EXISTS)
        .fetch_one(&mut conn)
        .await?;
    let version: Option<i64> = if has_journal {
        sqlx::query_scalar(APPLIED_VERSION)
            .fetch_one(&mut conn)
            .await?
    } else {
        None
    };
    conn.close().await?;
    version
        .map(|v| {
            u32::try_from(v).map_err(|_| SqliteStorageError::Decode(format!("schema version {v}")))
        })
        .transpose()
}

pub struct SqliteDatabaseFileSchema;

impl forge_storage::DatabaseFileSchema for SqliteDatabaseFileSchema {
    fn applied_schema_version(
        &self,
        database_file: &Path,
    ) -> Result<Option<u32>, forge_storage::StorageError> {
        read_applied_schema_version(database_file).map_err(forge_storage::StorageError::from)
    }
}
