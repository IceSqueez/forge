use std::io;
use std::path::Path;

use forge_storage::{StoredDatabase, StoredSchema};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use sqlx::{ConnectOptions, Connection};

use crate::error::SqliteStorageError;
use crate::migrations::{JournalState, classify};

const JOURNAL_TABLE_EXISTS: &str =
    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')";
const APPLIED_VERSION: &str = "SELECT MAX(version) FROM _sqlx_migrations";
const CREDENTIALS_TABLE_EXISTS: &str =
    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'credentials')";
const ANY_CREDENTIAL: &str = "SELECT EXISTS(SELECT 1 FROM credentials)";

pub fn read_applied_schema_version(path: &Path) -> Result<Option<u32>, SqliteStorageError> {
    if !database_file_exists(path)? {
        return Ok(None);
    }
    current_thread_runtime()?.block_on(read_version(path))
}

pub fn inspect_database_file(path: &Path) -> Result<Option<StoredDatabase>, SqliteStorageError> {
    if !database_file_exists(path)? {
        return Ok(None);
    }
    current_thread_runtime()?.block_on(inspect(path)).map(Some)
}

fn database_file_exists(path: &Path) -> Result<bool, SqliteStorageError> {
    path.try_exists()
        .map_err(|e| SqliteStorageError::Sqlx(sqlx::Error::Io(e)))
}

fn current_thread_runtime() -> Result<tokio::runtime::Runtime, SqliteStorageError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e: io::Error| SqliteStorageError::Sqlx(sqlx::Error::Io(e)))
}

async fn open_read_only(path: &Path) -> Result<SqliteConnection, SqliteStorageError> {
    Ok(SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .connect()
        .await?)
}

async fn read_version(path: &Path) -> Result<Option<u32>, SqliteStorageError> {
    let mut conn = open_read_only(path).await?;
    let version = applied_version(&mut conn).await;
    conn.close().await?;
    version
}

async fn applied_version(conn: &mut SqliteConnection) -> Result<Option<u32>, SqliteStorageError> {
    let has_journal: bool = sqlx::query_scalar(JOURNAL_TABLE_EXISTS)
        .fetch_one(&mut *conn)
        .await?;
    let version: Option<i64> = if has_journal {
        sqlx::query_scalar(APPLIED_VERSION)
            .fetch_one(&mut *conn)
            .await?
    } else {
        None
    };
    version
        .map(|v| {
            u32::try_from(v).map_err(|_| SqliteStorageError::Decode(format!("schema version {v}")))
        })
        .transpose()
}

async fn inspect(path: &Path) -> Result<StoredDatabase, SqliteStorageError> {
    let mut conn = open_read_only(path).await?;
    let inspected = inspect_connection(&mut conn).await;
    conn.close().await?;
    inspected
}

async fn inspect_connection(
    conn: &mut SqliteConnection,
) -> Result<StoredDatabase, SqliteStorageError> {
    let schema = match classify(&mut *conn).await {
        Ok(JournalState::Current) => match applied_version(&mut *conn).await? {
            Some(version) => StoredSchema::Current { version },
            None => StoredSchema::Unrecognized,
        },
        Ok(JournalState::LegacyComplete) => match applied_version(&mut *conn).await? {
            Some(version) => StoredSchema::AdoptableLegacy { version },
            None => StoredSchema::Unrecognized,
        },
        Ok(JournalState::LegacyPartial { found }) => StoredSchema::PreBaseline { found },
        Ok(JournalState::Empty) | Err(SqliteStorageError::UnrecognizedJournal { .. }) => {
            StoredSchema::Unrecognized
        }
        Err(other) => return Err(other),
    };
    Ok(StoredDatabase {
        schema,
        holds_credentials: holds_credentials(conn).await?,
    })
}

async fn holds_credentials(conn: &mut SqliteConnection) -> Result<bool, SqliteStorageError> {
    let has_table: bool = sqlx::query_scalar(CREDENTIALS_TABLE_EXISTS)
        .fetch_one(&mut *conn)
        .await?;
    if !has_table {
        return Ok(false);
    }
    Ok(sqlx::query_scalar(ANY_CREDENTIAL)
        .fetch_one(&mut *conn)
        .await?)
}

pub struct SqliteDatabaseFileSchema;

impl forge_storage::DatabaseFileSchema for SqliteDatabaseFileSchema {
    fn applied_schema_version(
        &self,
        database_file: &Path,
    ) -> Result<Option<u32>, forge_storage::StorageError> {
        read_applied_schema_version(database_file).map_err(forge_storage::StorageError::from)
    }

    fn inspect(
        &self,
        database_file: &Path,
    ) -> Result<Option<StoredDatabase>, forge_storage::StorageError> {
        inspect_database_file(database_file).map_err(forge_storage::StorageError::from)
    }
}
