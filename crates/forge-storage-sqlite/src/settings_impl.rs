use async_trait::async_trait;
use forge_storage::{
    RowClasses, RowGate, SettingDisclosure, SettingPortability, SettingRow, SettingsRepo,
    StorageError,
};

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
    async fn get_raw(&self, _gate: RowGate, key: &str) -> Result<Option<String>, StorageError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(self.db.reader())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(row.map(|(v,)| v))
    }

    async fn put_row(
        &self,
        _gate: RowGate,
        key: &str,
        value: &str,
        classes: RowClasses,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO settings (key, value, portability, disclosure) VALUES (?, ?, ?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, \
             portability = excluded.portability, disclosure = excluded.disclosure",
        )
        .bind(key)
        .bind(value)
        .bind(classes.portability.code())
        .bind(classes.disclosure.code())
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn load_rows(&self, gate: RowGate) -> Result<Vec<SettingRow>, StorageError> {
        #[derive(sqlx::FromRow)]
        struct ClassedRow {
            key: String,
            value: String,
            portability: String,
            disclosure: String,
        }

        let rows: Vec<ClassedRow> =
            sqlx::query_as("SELECT key, value, portability, disclosure FROM settings ORDER BY key")
                .fetch_all(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| {
                let portability =
                    SettingPortability::from_code(&row.portability).ok_or_else(|| {
                        SqliteStorageError::Decode(format!(
                            "unknown portability '{}'",
                            row.portability
                        ))
                    })?;
                let disclosure =
                    SettingDisclosure::from_code(&row.disclosure).ok_or_else(|| {
                        SqliteStorageError::Decode(format!(
                            "unknown disclosure '{}'",
                            row.disclosure
                        ))
                    })?;
                Ok(SettingRow::from_storage(
                    gate,
                    row.key,
                    row.value,
                    RowClasses {
                        portability,
                        disclosure,
                    },
                ))
            })
            .collect::<Result<Vec<_>, SqliteStorageError>>()
            .map_err(StorageError::from)
    }

    async fn delete_raw(&self, _gate: RowGate, key: &str) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }
}
