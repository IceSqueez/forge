use async_trait::async_trait;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use time::OffsetDateTime;

use crate::crypto;
use crate::epoch::{epoch_ms_now, from_epoch_ms};
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

pub struct SqliteCredentialsRepo {
    db: SqlitePools,
    key: [u8; 32],
}

impl SqliteCredentialsRepo {
    pub fn new_with_key(db: impl Into<SqlitePools>, key: [u8; 32]) -> Self {
        Self { db: db.into(), key }
    }

    pub(crate) async fn stored_count(&self) -> Result<u64, SqliteStorageError> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM credentials")
            .fetch_all(self.db.reader())
            .await?;
        let count = ids
            .iter()
            .filter(|id| !forge_storage::is_regenerable_credential(id))
            .count();
        Ok(u64::try_from(count).unwrap_or_default())
    }
}

#[async_trait]
impl CredentialsRepo for SqliteCredentialsRepo {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        let (ciphertext, nonce) =
            crypto::encrypt(&self.key, plaintext_bundle).map_err(StorageError::from)?;
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO credentials (id, encrypted, nonce, last_refresh)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                 encrypted    = excluded.encrypted,
                 nonce        = excluded.nonce,
                 last_refresh = excluded.last_refresh",
        )
        .bind(id.as_str())
        .bind(&ciphertext)
        .bind(&nonce)
        .bind(now_ms)
        .execute(self.db.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        #[derive(sqlx::FromRow)]
        struct CredentialRow {
            encrypted: Vec<u8>,
            nonce: Vec<u8>,
        }

        let row: Option<CredentialRow> =
            sqlx::query_as("SELECT encrypted, nonce FROM credentials WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let plaintext = crypto::decrypt(&self.key, &row.encrypted, &row.nonce)
            .map_err(|_| StorageError::Decryption)?;

        Ok(Some(plaintext))
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM credentials WHERE id = ?")
            .bind(id.as_str())
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM credentials ORDER BY id")
            .fetch_all(self.db.reader())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(rows
            .into_iter()
            .map(|(id,)| CredentialId::new(id))
            .collect())
    }

    async fn last_refresh(
        &self,
        id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        let ms: Option<i64> =
            sqlx::query_scalar("SELECT last_refresh FROM credentials WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        match ms {
            None => Ok(None),
            Some(ms) => from_epoch_ms(ms).map(Some).map_err(StorageError::from),
        }
    }

    async fn mark_refreshed(&self, id: &CredentialId) -> Result<(), StorageError> {
        let now_ms = epoch_ms_now();

        let result = sqlx::query("UPDATE credentials SET last_refresh = ? WHERE id = ?")
            .bind(now_ms)
            .bind(id.as_str())
            .execute(self.db.writer())
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        if result.rows_affected() == 0 {
            return Err(StorageError::NotFound {
                key: id.as_str().to_owned(),
            });
        }
        Ok(())
    }
}
