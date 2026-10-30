#[derive(Debug, thiserror::Error)]
pub enum SqliteStorageError {
    #[error("migration failed ({migration}): {reason}")]
    Migration { migration: String, reason: String },

    #[error("schema version mismatch: code expects {expected}, database is at {found}")]
    SchemaMismatch { expected: u32, found: u32 },

    #[error("database predates the baseline schema (legacy version {found})")]
    PreBaselineSchema { found: u32 },

    #[error("unrecognized migration journal: {reason}")]
    UnrecognizedJournal { reason: String },

    #[error("pre-baseline snapshot failed: {reason}")]
    AdoptionSnapshot { reason: String },

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
            SqliteStorageError::PreBaselineSchema { found } => Self::PreBaselineSchema { found },
            SqliteStorageError::UnrecognizedJournal { reason } => Self::Migration {
                migration: crate::migrations::JOURNAL_CHECK.to_owned(),
                reason,
            },
            SqliteStorageError::AdoptionSnapshot { reason } => Self::Migration {
                migration: crate::migrations::JOURNAL_CHECK.to_owned(),
                reason,
            },
            SqliteStorageError::Sqlx(inner) => {
                let reason = inner.to_string();
                let violated = inner.as_database_error().is_some_and(|database| {
                    database.is_unique_violation() || database.is_foreign_key_violation()
                });
                if violated {
                    Self::ConstraintViolated { reason }
                } else {
                    Self::Connection { reason }
                }
            }
            SqliteStorageError::Crypto { reason } => Self::Connection { reason },
            SqliteStorageError::KeyFile { reason } => Self::Connection { reason },
            SqliteStorageError::Decode(msg) => Self::Parse(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::StorageError;
    use sqlx::SqlitePool;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::SqliteStorageError;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for statement in [
            "PRAGMA foreign_keys = ON",
            "CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT UNIQUE)",
            "CREATE TABLE child (parent_id INTEGER NOT NULL REFERENCES parent (id))",
            "INSERT INTO parent (id, name) VALUES (1, 'taken')",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn failure_of(pool: &SqlitePool, statement: &'static str) -> StorageError {
        let error = sqlx::query(statement).execute(pool).await.unwrap_err();
        SqliteStorageError::Sqlx(error).into()
    }

    #[tokio::test]
    async fn unique_and_foreign_key_violations_convert_to_constraint_violated() {
        let pool = pool().await;
        for statement in [
            "INSERT INTO parent (id, name) VALUES (2, 'taken')",
            "INSERT INTO child (parent_id) VALUES (99)",
        ] {
            let error = failure_of(&pool, statement).await;

            assert!(
                matches!(error, StorageError::ConstraintViolated { .. }),
                "{statement}: {error:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_database_failure_that_breaks_no_constraint_stays_a_connection_error() {
        let error = failure_of(&pool().await, "SELECT * FROM no_such_table").await;

        assert!(
            matches!(error, StorageError::Connection { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_failure_without_a_database_answer_stays_a_connection_error() {
        let error: StorageError = SqliteStorageError::Sqlx(sqlx::Error::PoolTimedOut).into();

        assert!(
            matches!(error, StorageError::Connection { .. }),
            "{error:?}"
        );
    }
}
