use async_trait::async_trait;
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use time::OffsetDateTime;

use crate::crypto;
use crate::error::SqliteStorageError;

pub struct SqliteCredentialsRepo {
    pool: sqlx::SqlitePool,
    key: [u8; 32],
}

impl SqliteCredentialsRepo {
    pub fn new(pool: sqlx::SqlitePool) -> Result<Self, SqliteStorageError> {
        let key = crypto::load_or_create_key()?;
        Ok(Self { pool, key })
    }

    #[doc(hidden)]
    pub fn new_with_key(pool: sqlx::SqlitePool, key: [u8; 32]) -> Self {
        Self { pool, key }
    }
}

fn epoch_ms_now() -> i64 {
    let now = OffsetDateTime::now_utc();
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
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
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        let row: Option<(Vec<u8>, Vec<u8>)> =
            sqlx::query_as("SELECT encrypted, nonce FROM credentials WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        let Some((ciphertext, nonce)) = row else {
            return Ok(None);
        };

        let plaintext =
            crypto::decrypt(&self.key, &ciphertext, &nonce).map_err(StorageError::from)?;

        Ok(Some(plaintext))
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM credentials WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM credentials ORDER BY id")
            .fetch_all(&self.pool)
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
                .fetch_optional(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        match ms {
            None => Ok(None),
            Some(ms) => from_epoch_ms(ms).map(Some).map_err(StorageError::from),
        }
    }

    async fn mark_refreshed(&self, id: &CredentialId) -> Result<(), StorageError> {
        let now_ms = epoch_ms_now();

        sqlx::query("UPDATE credentials SET last_refresh = ? WHERE id = ?")
            .bind(now_ms)
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }
}
