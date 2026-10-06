use std::str::FromStr;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::SqliteStorageError;

pub(crate) const BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const JOURNAL_SIZE_LIMIT_BYTES: &str = "67108864";
const SHUTDOWN_CHECKPOINT_WAIT: Duration = Duration::from_millis(8);
const WRITER_CONNECTIONS: u32 = 1;
const SHARED_POOL_CONNECTIONS: u32 = 4;
const MIN_READER_CONNECTIONS: u32 = 2;
const MAX_READER_CONNECTIONS: u32 = 8;

#[derive(Clone, Debug)]
pub struct SqlitePools {
    reader: SqlitePool,
    writer: SqlitePool,
    checkpointer: Option<SqlitePool>,
}

impl SqlitePools {
    pub fn reader(&self) -> &SqlitePool {
        &self.reader
    }

    pub fn writer(&self) -> &SqlitePool {
        &self.writer
    }

    pub(crate) fn checkpointer(&self) -> Option<&SqlitePool> {
        self.checkpointer.as_ref()
    }

    pub async fn close(&self) {
        self.truncate_wal().await;
        if let Some(checkpointer) = &self.checkpointer {
            checkpointer.close().await;
        }
        self.reader.close().await;
        self.writer.close().await;
    }
}

impl SqlitePools {
    async fn truncate_wal(&self) {
        let Ok(mut conn) = self.writer.acquire().await else {
            return;
        };
        let bounded = sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA busy_timeout = {}",
            SHUTDOWN_CHECKPOINT_WAIT.as_millis()
        )))
        .execute(&mut *conn)
        .await;
        if bounded.is_err() {
            return;
        }
        match sqlx::query_as::<_, (i64, i64, i64)>("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(&mut *conn)
            .await
        {
            Ok((0, _, _)) => {}
            Ok(_) => tracing::warn!("shutdown WAL truncate was outlasted by readers"),
            Err(e) => tracing::warn!(error = %e, "shutdown WAL truncate failed"),
        }
        conn.close_on_drop();
    }
}

impl From<SqlitePool> for SqlitePools {
    fn from(pool: SqlitePool) -> Self {
        Self {
            reader: pool.clone(),
            writer: pool,
            checkpointer: None,
        }
    }
}

fn options(url: &str) -> Result<SqliteConnectOptions, SqliteStorageError> {
    Ok(SqliteConnectOptions::from_str(url)
        .map_err(SqliteStorageError::Sqlx)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(BUSY_TIMEOUT)
        .pragma("journal_size_limit", JOURNAL_SIZE_LIMIT_BYTES)
        .foreign_keys(true)
        .create_if_missing(true))
}

pub async fn connect(url: &str) -> Result<SqlitePool, SqliteStorageError> {
    SqlitePoolOptions::new()
        .max_connections(SHARED_POOL_CONNECTIONS)
        .connect_with(options(url)?)
        .await
        .map_err(SqliteStorageError::Sqlx)
}

pub async fn connect_pools(url: &str) -> Result<SqlitePools, SqliteStorageError> {
    let opts = options(url)?;
    let writer = SqlitePoolOptions::new()
        .max_connections(WRITER_CONNECTIONS)
        .connect_with(opts.clone().pragma("wal_autocheckpoint", "0"))
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    let checkpointer = SqlitePoolOptions::new()
        .max_connections(WRITER_CONNECTIONS)
        .connect_with(opts.clone())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    let reader = SqlitePoolOptions::new()
        .max_connections(reader_connections())
        .connect_with(opts.pragma("query_only", "ON"))
        .await
        .map_err(SqliteStorageError::Sqlx)?;

    Ok(SqlitePools {
        reader,
        writer,
        checkpointer: Some(checkpointer),
    })
}

fn reader_connections() -> u32 {
    std::thread::available_parallelism()
        .map_or(MIN_READER_CONNECTIONS, |cores| {
            u32::try_from(cores.get()).unwrap_or(MAX_READER_CONNECTIONS)
        })
        .clamp(MIN_READER_CONNECTIONS, MAX_READER_CONNECTIONS)
}
