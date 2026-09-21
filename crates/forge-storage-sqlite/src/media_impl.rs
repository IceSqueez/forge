use std::io::Read as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use forge_storage::{
    MEDIA_BLOB_HARD_CEILING_BYTES, MEDIA_CONTENT_DIGEST_BYTES, MediaBlob, MediaBlobId, MediaFormat,
    MediaReferrer, MediaReferrerKind, MediaRepo, StorageError, accept_media, sanitize_label, sniff,
};
use rand::rand_core::Rng as _;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

const BLOB_FILE_EXTENSION_SEPARATOR: char = '.';
const INCOMING_FILE_PREFIX: &str = ".incoming-";
const INCOMING_NONCE_BYTES: usize = 8;
const IMPORT_SNIFF_WINDOW_BYTES: u64 = 4096;

fn content_digest(bytes: &[u8]) -> [u8; MEDIA_CONTENT_DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn blob_path(root: &Path, id: &MediaBlobId, format: MediaFormat) -> PathBuf {
    root.join(format!(
        "{}{}{}",
        id.as_str(),
        BLOB_FILE_EXTENSION_SEPARATOR,
        format.as_str()
    ))
}

fn incoming_path(root: &Path) -> PathBuf {
    use std::fmt::Write as _;

    let mut nonce = [0u8; INCOMING_NONCE_BYTES];
    rand::rng().fill_bytes(&mut nonce);

    let mut name = String::from(INCOMING_FILE_PREFIX);
    for byte in nonce {
        let _ = write!(name, "{byte:02x}");
    }
    root.join(name)
}

fn write_blob(root: &Path, format: MediaFormat, bytes: &[u8]) -> std::io::Result<MediaBlobId> {
    let id = MediaBlobId::from_digest(&content_digest(bytes));
    let final_path = blob_path(root, &id, format);

    std::fs::create_dir_all(root)?;
    if final_path.exists() {
        return Ok(id);
    }

    let staging = incoming_path(root);
    {
        let mut file = std::fs::File::create(&staging)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    if let Err(error) = std::fs::rename(&staging, &final_path) {
        let _ = std::fs::remove_file(&staging);
        if !final_path.exists() {
            return Err(error);
        }
    }

    Ok(id)
}

fn read_import(source: &Path) -> Result<(String, Vec<u8>), StorageError> {
    let raw_label = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();

    let mut file = std::fs::File::open(source)?;
    let declared_len = file.metadata()?.len();

    let mut bytes = Vec::new();
    std::io::Read::take(&mut file, IMPORT_SNIFF_WINDOW_BYTES).read_to_end(&mut bytes)?;

    let Some(format) = sniff(&bytes) else {
        return Err(StorageError::MediaUnsupported {
            label: sanitize_label(&raw_label),
        });
    };

    let kind = format.kind();
    let limit = kind.max_blob_bytes();
    if declared_len > limit {
        return Err(StorageError::MediaTooLarge {
            label: sanitize_label(&raw_label),
            size: declared_len,
            limit,
            kind,
        });
    }

    let remaining = MEDIA_BLOB_HARD_CEILING_BYTES.saturating_sub(bytes.len() as u64) + 1;
    std::io::Read::take(&mut file, remaining).read_to_end(&mut bytes)?;

    Ok((raw_label, bytes))
}

fn task_failure(error: tokio::task::JoinError) -> StorageError {
    StorageError::Io(std::io::Error::other(error))
}

fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / 1_000_000) as i64
}

fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}

#[derive(sqlx::FromRow)]
struct MediaBlobRow {
    id: String,
    format: String,
    byte_size: i64,
    label: String,
    imported_at: i64,
}

fn decode_row(row: MediaBlobRow) -> Result<MediaBlob, SqliteStorageError> {
    let format = MediaFormat::parse(&row.format).ok_or_else(|| {
        SqliteStorageError::Decode(format!("unknown media format '{}'", row.format))
    })?;

    Ok(MediaBlob {
        id: MediaBlobId::from_stored(row.id),
        format,
        byte_size: row.byte_size.max(0) as u64,
        label: row.label,
        imported_at: from_epoch_ms(row.imported_at)?,
    })
}

#[derive(sqlx::FromRow)]
struct MediaReferenceRow {
    referrer_kind: String,
    referrer_id: String,
    slot: String,
}

fn decode_referrer(row: MediaReferenceRow) -> Result<MediaReferrer, SqliteStorageError> {
    let kind = MediaReferrerKind::parse(&row.referrer_kind).ok_or_else(|| {
        SqliteStorageError::Decode(format!("unknown media referrer '{}'", row.referrer_kind))
    })?;

    Ok(MediaReferrer::new(kind, row.referrer_id, row.slot))
}

pub struct SqliteMediaRepo {
    pool: sqlx::SqlitePool,
    root: PathBuf,
}

impl SqliteMediaRepo {
    pub fn new(pool: sqlx::SqlitePool, root: PathBuf) -> Self {
        Self { pool, root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    async fn index(
        &self,
        id: &MediaBlobId,
        format: MediaFormat,
        byte_size: u64,
        label: &str,
    ) -> Result<MediaBlob, StorageError> {
        sqlx::query(
            "INSERT INTO media_blobs (id, format, byte_size, label, imported_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(id.as_str())
        .bind(format.as_str())
        .bind(byte_size as i64)
        .bind(label)
        .bind(to_epoch_ms(OffsetDateTime::now_utc()))
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        self.get(id).await?.ok_or_else(|| StorageError::NotFound {
            key: id.as_str().to_owned(),
        })
    }
}

#[async_trait]
impl MediaRepo for SqliteMediaRepo {
    async fn store(&self, label: &str, bytes: Vec<u8>) -> Result<MediaBlob, StorageError> {
        let accepted = accept_media(label, &bytes)?;
        let byte_size = bytes.len() as u64;
        let format = accepted.format;
        let root = self.root.clone();

        let id = tokio::task::spawn_blocking(move || write_blob(&root, format, &bytes))
            .await
            .map_err(task_failure)??;

        self.index(&id, format, byte_size, &accepted.label).await
    }

    async fn import_file(&self, source: &Path) -> Result<MediaBlob, StorageError> {
        let source = source.to_path_buf();
        let (label, bytes) = tokio::task::spawn_blocking(move || read_import(&source))
            .await
            .map_err(task_failure)??;

        self.store(&label, bytes).await
    }

    async fn get(&self, id: &MediaBlobId) -> Result<Option<MediaBlob>, StorageError> {
        let row: Option<MediaBlobRow> = sqlx::query_as(
            "SELECT id, format, byte_size, label, imported_at FROM media_blobs WHERE id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn list(&self) -> Result<Vec<MediaBlob>, StorageError> {
        let rows: Vec<MediaBlobRow> = sqlx::query_as(
            "SELECT id, format, byte_size, label, imported_at
             FROM media_blobs
             ORDER BY imported_at DESC, label ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn total_bytes(&self) -> Result<u64, StorageError> {
        let (total,): (i64,) =
            sqlx::query_as("SELECT COALESCE(SUM(byte_size), 0) FROM media_blobs")
                .fetch_one(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        Ok(total.max(0) as u64)
    }

    async fn resolve(&self, id: &MediaBlobId) -> Result<PathBuf, StorageError> {
        let blob = self.get(id).await?.ok_or_else(|| StorageError::NotFound {
            key: id.as_str().to_owned(),
        })?;

        let path = blob_path(&self.root, id, blob.format);
        let probe = path.clone();
        let present = tokio::task::spawn_blocking(move || probe.is_file())
            .await
            .map_err(task_failure)?;

        if !present {
            return Err(StorageError::NotFound {
                key: id.as_str().to_owned(),
            });
        }

        Ok(path)
    }

    async fn read(&self, id: &MediaBlobId) -> Result<Vec<u8>, StorageError> {
        let path = self.resolve(id).await?;

        tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(task_failure)?
            .map_err(StorageError::Io)
    }

    async fn delete(&self, id: &MediaBlobId) -> Result<bool, StorageError> {
        let holders = self.referrers(id).await?;
        if !holders.is_empty() {
            return Err(StorageError::MediaReferenced {
                referrer_count: holders.len() as u32,
            });
        }

        let Some(blob) = self.get(id).await? else {
            return Ok(false);
        };

        let removed = sqlx::query("DELETE FROM media_blobs WHERE id = ?")
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(SqliteStorageError::Sqlx)?;

        if removed.rows_affected() == 0 {
            return Ok(false);
        }

        let path = blob_path(&self.root, id, blob.format);
        let discarded = tokio::task::spawn_blocking(move || std::fs::remove_file(path))
            .await
            .map_err(task_failure)?;

        if let Err(error) = discarded {
            tracing::warn!(blob = %id, %error, "managed media file left behind after index removal");
        }

        Ok(true)
    }

    async fn retain(&self, referrer: &MediaReferrer, id: &MediaBlobId) -> Result<(), StorageError> {
        if self.get(id).await?.is_none() {
            return Err(StorageError::NotFound {
                key: id.as_str().to_owned(),
            });
        }

        sqlx::query(
            "INSERT INTO media_references (referrer_kind, referrer_id, slot, blob_id)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(referrer_kind, referrer_id, slot)
             DO UPDATE SET blob_id = excluded.blob_id",
        )
        .bind(referrer.kind.as_str())
        .bind(&referrer.id)
        .bind(&referrer.slot)
        .bind(id.as_str())
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(())
    }

    async fn release(&self, referrer: &MediaReferrer) -> Result<bool, StorageError> {
        let result = sqlx::query(
            "DELETE FROM media_references
             WHERE referrer_kind = ? AND referrer_id = ? AND slot = ?",
        )
        .bind(referrer.kind.as_str())
        .bind(&referrer.id)
        .bind(&referrer.slot)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected() > 0)
    }

    async fn release_all(
        &self,
        kind: MediaReferrerKind,
        referrer_id: &str,
    ) -> Result<u64, StorageError> {
        let result =
            sqlx::query("DELETE FROM media_references WHERE referrer_kind = ? AND referrer_id = ?")
                .bind(kind.as_str())
                .bind(referrer_id)
                .execute(&self.pool)
                .await
                .map_err(SqliteStorageError::Sqlx)?;

        Ok(result.rows_affected())
    }

    async fn referrers(&self, id: &MediaBlobId) -> Result<Vec<MediaReferrer>, StorageError> {
        let rows: Vec<MediaReferenceRow> = sqlx::query_as(
            "SELECT referrer_kind, referrer_id, slot
             FROM media_references
             WHERE blob_id = ?
             ORDER BY referrer_kind ASC, referrer_id ASC, slot ASC",
        )
        .bind(id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_referrer(row).map_err(StorageError::from))
            .collect()
    }

    async fn blob_of(&self, referrer: &MediaReferrer) -> Result<Option<MediaBlobId>, StorageError> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT blob_id FROM media_references
             WHERE referrer_kind = ? AND referrer_id = ? AND slot = ?",
        )
        .bind(referrer.kind.as_str())
        .bind(&referrer.id)
        .bind(&referrer.slot)
        .fetch_optional(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        Ok(row.map(|(blob_id,)| MediaBlobId::from_stored(blob_id)))
    }
}
