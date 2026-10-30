use forge_storage::test_support::StoredSettingRow;
use forge_storage::{RowClasses, SettingDisclosure, SettingPortability};

use crate::SqliteBackend;
use crate::error::SqliteStorageError;

#[derive(sqlx::FromRow)]
struct RawSettingRow {
    key: String,
    value: String,
    portability: String,
    disclosure: String,
}

impl RawSettingRow {
    fn into_stored(self) -> Result<StoredSettingRow, SqliteStorageError> {
        let portability = SettingPortability::from_code(&self.portability).ok_or_else(|| {
            SqliteStorageError::Decode(format!("unknown portability '{}'", self.portability))
        })?;
        let disclosure = SettingDisclosure::from_code(&self.disclosure).ok_or_else(|| {
            SqliteStorageError::Decode(format!("unknown disclosure '{}'", self.disclosure))
        })?;
        Ok(StoredSettingRow {
            key: self.key,
            value: self.value,
            classes: RowClasses {
                portability,
                disclosure,
            },
        })
    }
}

impl SqliteBackend {
    pub async fn seed_foreign_setting(
        &self,
        key: &str,
        value: &str,
        classes: Option<RowClasses>,
    ) -> Result<(), SqliteStorageError> {
        let query = match classes {
            Some(classes) => sqlx::query(
                "INSERT INTO settings (key, value, portability, disclosure) VALUES (?, ?, ?, ?) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, \
                 portability = excluded.portability, disclosure = excluded.disclosure",
            )
            .bind(key)
            .bind(value)
            .bind(classes.portability.code())
            .bind(classes.disclosure.code()),
            None => sqlx::query(
                "INSERT INTO settings (key, value) VALUES (?, ?) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(key)
            .bind(value),
        };
        query
            .execute(self.pool.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    pub async fn stored_setting_rows(&self) -> Result<Vec<StoredSettingRow>, SqliteStorageError> {
        let rows: Vec<RawSettingRow> =
            sqlx::query_as("SELECT key, value, portability, disclosure FROM settings ORDER BY key")
                .fetch_all(self.pool.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;
        rows.into_iter().map(RawSettingRow::into_stored).collect()
    }
}
