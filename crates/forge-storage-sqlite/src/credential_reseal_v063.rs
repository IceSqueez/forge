use crate::codec::crypto;
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;

#[derive(sqlx::FromRow)]
struct StoredCredential {
    id: String,
    encrypted: Vec<u8>,
    nonce: Vec<u8>,
}

pub(crate) async fn reseal_unbound_credentials(db: &SqlitePools, key: &[u8; 32]) {
    let rows: Vec<StoredCredential> =
        match sqlx::query_as("SELECT id, encrypted, nonce FROM credentials")
            .fetch_all(db.reader())
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "stored credentials could not be listed for resealing");
                return;
            }
        };

    let mut resealed = 0_usize;
    for row in &rows {
        if crypto::decrypt(key, &row.id, &row.encrypted, &row.nonce).is_ok() {
            continue;
        }
        match reseal_row(db, key, row).await {
            Ok(true) => resealed += 1,
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(id = %row.id, %error, "credential could not be resealed");
            }
        }
    }
    if resealed > 0 {
        tracing::info!(resealed, "credentials from the previous release resealed");
    }
}

async fn reseal_row(
    db: &SqlitePools,
    key: &[u8; 32],
    row: &StoredCredential,
) -> Result<bool, SqliteStorageError> {
    let Ok(plaintext) = crypto::decrypt(key, "", &row.encrypted, &row.nonce) else {
        return Ok(false);
    };
    let (ciphertext, nonce) = crypto::encrypt(key, &row.id, &plaintext)?;
    crypto::decrypt(key, &row.id, &ciphertext, &nonce)?;

    let updated = sqlx::query(
        "UPDATE credentials SET encrypted = ?, nonce = ?
         WHERE id = ? AND encrypted = ? AND nonce = ?",
    )
    .bind(&ciphertext)
    .bind(&nonce)
    .bind(&row.id)
    .bind(&row.encrypted)
    .bind(&row.nonce)
    .execute(db.writer())
    .await?;
    Ok(updated.rows_affected() > 0)
}
