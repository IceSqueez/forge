use std::collections::HashMap;

use async_trait::async_trait;
use forge_storage::{SettingsRepo, StorageError};

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

pub struct SqliteSettingsRepo {
    db: SqlitePools,
}

impl SqliteSettingsRepo {
    pub fn new(db: impl Into<SqlitePools>) -> Self {
        Self { db: db.into() }
    }
}

#[async_trait]
impl SettingsRepo for SqliteSettingsRepo {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(self.db.reader())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(row.map(|(v,)| v))
    }

    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        #[derive(sqlx::FromRow)]
        struct SettingRow {
            key: String,
            value: String,
        }

        let rows: Vec<SettingRow> = sqlx::query_as("SELECT key, value FROM settings")
            .fetch_all(self.db.reader())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(rows.into_iter().map(|row| (row.key, row.value)).collect())
    }
}
