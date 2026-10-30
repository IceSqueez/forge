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

    pub(crate) async fn bind_unbound_rows(&self) {
        let rows: Vec<UnboundRow> = match sqlx::query_as(
            "SELECT id, encrypted, nonce FROM credentials WHERE bound_to_id = 0",
        )
        .fetch_all(self.db.writer())
        .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "unbound credentials could not be listed");
                return;
            }
        };

        let mut left_unbound = 0_usize;
        for row in &rows {
            if let Err(error) = self.bind_row(row).await {
                left_unbound += 1;
                tracing::warn!(id = %row.id, %error, "credential left in its previous format");
            }
        }
        if !rows.is_empty() {
            tracing::info!(
                rewritten = rows.len() - left_unbound,
                left_unbound,
                "credentials bound to their ids"
            );
        }
    }

    async fn bind_row(&self, row: &UnboundRow) -> Result<(), SqliteStorageError> {
        let plaintext = crypto::decrypt(&self.key, &row.encrypted, &row.nonce)?;
        let (ciphertext, nonce) = crypto::encrypt_for(&self.key, &row.id, &plaintext)?;
        let readable_back = crypto::decrypt_for(&self.key, &row.id, &ciphertext, &nonce)?;
        if readable_back != plaintext {
            return Err(SqliteStorageError::Crypto {
                reason: "rewritten credential did not read back".to_owned(),
            });
        }

        sqlx::query(
            "UPDATE credentials SET encrypted = ?, nonce = ?, bound_to_id = 1
             WHERE id = ? AND bound_to_id = 0 AND encrypted = ? AND nonce = ?",
        )
        .bind(&ciphertext)
        .bind(&nonce)
        .bind(&row.id)
        .bind(&row.encrypted)
        .bind(&row.nonce)
        .execute(self.db.writer())
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct UnboundRow {
    id: String,
    encrypted: Vec<u8>,
    nonce: Vec<u8>,
}

#[async_trait]
impl CredentialsRepo for SqliteCredentialsRepo {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        let (ciphertext, nonce) = crypto::encrypt_for(&self.key, id.as_str(), plaintext_bundle)
            .map_err(StorageError::from)?;
        let now_ms = epoch_ms_now();

        sqlx::query(
            "INSERT INTO credentials (id, encrypted, nonce, last_refresh, bound_to_id)
             VALUES (?, ?, ?, ?, 1)
             ON CONFLICT(id) DO UPDATE SET
                 encrypted    = excluded.encrypted,
                 nonce        = excluded.nonce,
                 last_refresh = excluded.last_refresh,
                 bound_to_id  = 1",
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
            bound_to_id: i64,
        }

        let row: Option<CredentialRow> =
            sqlx::query_as("SELECT encrypted, nonce, bound_to_id FROM credentials WHERE id = ?")
                .bind(id.as_str())
                .fetch_optional(self.db.reader())
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let plaintext = if row.bound_to_id != 0 {
            crypto::decrypt_for(&self.key, id.as_str(), &row.encrypted, &row.nonce)
        } else {
            crypto::decrypt(&self.key, &row.encrypted, &row.nonce)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [0x3e; 32];
    const ID: &str = "twitch:bot";

    async fn repo_with_legacy_row() -> (SqliteCredentialsRepo, sqlx::SqlitePool, UnboundRow) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::apply_migrations(&pool).await.unwrap();
        let (encrypted, nonce) = crypto::encrypt(&KEY, "read_before_the_race").unwrap();
        sqlx::query(
            "INSERT INTO credentials (id, encrypted, nonce, last_refresh) VALUES (?, ?, ?, 0)",
        )
        .bind(ID)
        .bind(&encrypted)
        .bind(&nonce)
        .execute(&pool)
        .await
        .unwrap();
        let row = UnboundRow {
            id: ID.to_owned(),
            encrypted,
            nonce,
        };
        let repo = SqliteCredentialsRepo::new_with_key(pool.clone(), KEY);
        (repo, pool, row)
    }

    #[tokio::test]
    async fn a_store_landing_after_the_read_is_not_overwritten_by_the_rewrite() {
        let (repo, _pool, read_before_the_store) = repo_with_legacy_row().await;
        let id = CredentialId::new(ID);
        repo.store(&id, "rotated_token").await.unwrap();

        repo.bind_row(&read_before_the_store).await.unwrap();

        assert_eq!(
            repo.load(&id).await.unwrap().as_deref(),
            Some("rotated_token")
        );
    }

    #[tokio::test]
    async fn a_legacy_write_landing_after_the_read_is_not_overwritten_by_the_rewrite() {
        let (repo, pool, read_before_the_write) = repo_with_legacy_row().await;
        let (encrypted, nonce) = crypto::encrypt(&KEY, "rotated_token").unwrap();
        sqlx::query("UPDATE credentials SET encrypted = ?, nonce = ? WHERE id = ?")
            .bind(&encrypted)
            .bind(&nonce)
            .bind(ID)
            .execute(&pool)
            .await
            .unwrap();

        repo.bind_row(&read_before_the_write).await.unwrap();

        assert_eq!(
            repo.load(&CredentialId::new(ID)).await.unwrap().as_deref(),
            Some("rotated_token")
        );
    }
}
