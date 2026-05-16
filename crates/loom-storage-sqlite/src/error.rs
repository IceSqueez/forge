#[derive(Debug, thiserror::Error)]
pub enum SqliteStorageError {
    #[error("migration failed ({migration}): {reason}")]
    Migration { migration: String, reason: String },

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error("crypto error: {reason}")]
    Crypto { reason: String },

    #[error("keyring error: {reason}")]
    Keyring { reason: String },
}
