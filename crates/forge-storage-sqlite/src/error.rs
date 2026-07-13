#[derive(Debug, thiserror::Error)]
pub enum SqliteStorageError {
    #[error("migration failed ({migration}): {reason}")]
    Migration { migration: String, reason: String },

    #[error("schema version mismatch: code expects {expected}, database is at {found}")]
    SchemaMismatch { expected: u32, found: u32 },

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error("crypto error: {reason}")]
    Crypto { reason: String },

    #[error("key file error: {reason}")]
    KeyFile { reason: String },

    #[error("decode error: {0}")]
    Decode(String),
}

impl From<SqliteStorageError> for forge_storage::StorageError {
    fn from(e: SqliteStorageError) -> Self {
        match e {
            SqliteStorageError::Migration { migration, reason } => {
                Self::Migration { migration, reason }
            }
            SqliteStorageError::SchemaMismatch { expected, found } => {
                Self::SchemaMismatch { expected, found }
            }
            SqliteStorageError::Sqlx(inner) => Self::Connection {
                reason: inner.to_string(),
            },
            SqliteStorageError::Crypto { reason } => Self::Connection { reason },
            SqliteStorageError::KeyFile { reason } => Self::Connection { reason },
            SqliteStorageError::Decode(msg) => Self::Parse(msg),
        }
    }
}
