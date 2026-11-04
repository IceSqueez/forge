use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use forge_storage::{
    MEDIA_CONTENT_DIGEST_BYTES, MediaBlob, MediaBlobId, MediaFormat, MediaKind, MediaReferrer,
    MediaReferrerKind, MediaRepo, StorageError, accept_media, library_referrer, sanitize_label,
    sniff,
};
use rand::rand_core::Rng as _;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use crate::codec::epoch::{from_epoch_ms, to_epoch_ms};
use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;
use crate::repos::media_orphans::{MediaFiles, ReleasedBlobs};

macro_rules! media_blob_columns {
    () => {
        "id, format, byte_size, label, imported_at"
    };
}

const BLOB_FILE_EXTENSION_SEPARATOR: char = '.';
const INCOMING_FILE_PREFIX: &str = ".incoming-";
const INCOMING_NONCE_BYTES: usize = 8;
const IMPORT_SNIFF_WINDOW_BYTES: u64 = 4096;
const STALE_FILE_AGE: Duration = Duration::from_hours(1);

fn content_digest(bytes: &[u8]) -> [u8; MEDIA_CONTENT_DIGEST_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

pub(crate) fn blob_path(root: &Path, id: &MediaBlobId, format: MediaFormat) -> PathBuf {
    root.join(format!(
        "{}{}{}",
        id.as_str(),
        BLOB_FILE_EXTENSION_SEPARATOR,
        format.as_str()
    ))
}

fn incoming_path(root: &Path) -> PathBuf {
    let mut nonce = [0u8; INCOMING_NONCE_BYTES];
    rand::rng().fill_bytes(&mut nonce);

    root.join(format!(
        "{INCOMING_FILE_PREFIX}{}",
        forge_types::to_lower_hex(&nonce)
    ))
}

fn write_blob(root: &Path, format: MediaFormat, bytes: &[u8]) -> std::io::Result<MediaBlobId> {
    let id = MediaBlobId::from_digest(&content_digest(bytes));
    place_blob(root, &id, format, bytes)?;
    Ok(id)
}

fn place_blob(
    root: &Path,
    id: &MediaBlobId,
    format: MediaFormat,
    bytes: &[u8],
) -> std::io::Result<()> {
    let final_path = blob_path(root, id, format);

    std::fs::create_dir_all(root)?;
    if final_path.exists() {
        return Ok(());
    }

    let staging = incoming_path(root);
    {
        let mut file = std::fs::File::create(&staging)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }

    if let Err(error) = std::fs::rename(&staging, &final_path) {
        if let Err(error) = std::fs::remove_file(&staging) {
            tracing::debug!(%error, path = %staging.display(), "staged media file could not be removed");
        }
        if !final_path.exists() {
            return Err(error);
        }
    }
    forge_durable::sync_dir(root)
}

fn is_stale_file(entry: &std::fs::DirEntry, now: SystemTime) -> bool {
    entry
        .metadata()
        .ok()
        .filter(std::fs::Metadata::is_file)
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age >= STALE_FILE_AGE)
}

fn is_stale_incoming(entry: &std::fs::DirEntry, now: SystemTime) -> bool {
    let is_incoming = entry
        .file_name()
        .to_str()
        .is_some_and(|name| name.starts_with(INCOMING_FILE_PREFIX));
    is_incoming && is_stale_file(entry, now)
}

fn is_blob_file_name(name: &str) -> bool {
    name.rsplit_once(BLOB_FILE_EXTENSION_SEPARATOR)
        .is_some_and(|(id, format)| {
            MediaFormat::parse(format).is_some() && MediaBlobId::from_stored(id).is_ok()
        })
}

fn is_stale_unindexed_blob(
    entry: &std::fs::DirEntry,
    indexed: &HashSet<String>,
    now: SystemTime,
) -> bool {
    let name = entry.file_name();
    name.to_str()
        .is_some_and(|name| is_blob_file_name(name) && !indexed.contains(name))
        && is_stale_file(entry, now)
}

fn remove_entries(
    root: &Path,
    is_removable: impl Fn(&std::fs::DirEntry, SystemTime) -> bool,
) -> std::io::Result<usize> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };

    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        if !is_removable(&entry, now) {
            continue;
        }
        match std::fs::remove_file(entry.path()) {
            Ok(()) => removed += 1,
            Err(error) => tracing::warn!(%error, "stale media file could not be removed"),
        }
    }
    Ok(removed)
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
        return Err(StorageError::MediaRefused(
            forge_storage::MediaRefusal::Unsupported {
                label: sanitize_label(&raw_label),
            },
        ));
    };

    let kind = format.kind();
    let limit = kind.max_blob_bytes();
    if declared_len > limit {
        return Err(StorageError::MediaRefused(
            forge_storage::MediaRefusal::TooLarge {
                label: sanitize_label(&raw_label),
                size: declared_len,
                limit,
                kind,
            },
        ));
    }

    let remaining = (limit + 1).saturating_sub(bytes.len() as u64);
    std::io::Read::take(&mut file, remaining).read_to_end(&mut bytes)?;

    Ok((raw_label, bytes))
}

pub(crate) fn task_failure(error: tokio::task::JoinError) -> StorageError {
    StorageError::Io(std::io::Error::other(error))
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
        id: MediaBlobId::from_stored(row.id)
            .map_err(|e| SqliteStorageError::Decode(e.to_string()))?,
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

pub(crate) async fn retain_reference(
    conn: &mut sqlx::SqliteConnection,
    referrer: &MediaReferrer,
    id: &MediaBlobId,
    released: &mut ReleasedBlobs,
) -> Result<(), StorageError> {
    let known: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM media_blobs WHERE id = ?")
        .bind(id.as_str())
        .fetch_optional(&mut *conn)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
    if known.is_none() {
        return Err(StorageError::NotFound {
            key: id.as_str().to_owned(),
        });
    }

    let replaced: Option<(String,)> = sqlx::query_as(
        "SELECT blob_id FROM media_references
         WHERE referrer_kind = ? AND referrer_id = ? AND slot = ?",
    )
    .bind(referrer.kind.as_str())
    .bind(&referrer.id)
    .bind(&referrer.slot)
    .fetch_optional(&mut *conn)
    .await
    .map_err(SqliteStorageError::Sqlx)?;

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
    .execute(&mut *conn)
    .await
    .map_err(SqliteStorageError::Sqlx)?;

    if let Some((previous,)) = replaced.filter(|(previous,)| previous != id.as_str()) {
        released.note(previous);
    }
    Ok(())
}

pub(crate) async fn release_reference(
    conn: &mut sqlx::SqliteConnection,
    referrer: &MediaReferrer,
    released: &mut ReleasedBlobs,
) -> Result<bool, StorageError> {
    let dropped: Option<(String,)> = sqlx::query_as(
        "DELETE FROM media_references
         WHERE referrer_kind = ? AND referrer_id = ? AND slot = ?
         RETURNING blob_id",
    )
    .bind(referrer.kind.as_str())
    .bind(&referrer.id)
    .bind(&referrer.slot)
    .fetch_optional(&mut *conn)
    .await
    .map_err(SqliteStorageError::Sqlx)?;

    Ok(dropped.map(|(blob_id,)| released.note(blob_id)).is_some())
}

pub(crate) async fn release_all_references(
    conn: &mut sqlx::SqliteConnection,
    kind: MediaReferrerKind,
    referrer_id: &str,
    released: &mut ReleasedBlobs,
) -> Result<u64, StorageError> {
    let dropped: Vec<(String,)> = sqlx::query_as(
        "DELETE FROM media_references WHERE referrer_kind = ? AND referrer_id = ?
         RETURNING blob_id",
    )
    .bind(kind.as_str())
    .bind(referrer_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(SqliteStorageError::Sqlx)?;

    let count = dropped.len() as u64;
    for (blob_id,) in dropped {
        released.note(blob_id);
    }
    Ok(count)
}

async fn hold_in_library(
    conn: &mut sqlx::SqliteConnection,
    id: &MediaBlobId,
) -> Result<(), StorageError> {
    let referrer = library_referrer(id);
    sqlx::query(
        "INSERT INTO media_references (referrer_kind, referrer_id, slot, blob_id)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(referrer_kind, referrer_id, slot) DO NOTHING",
    )
    .bind(referrer.kind.as_str())
    .bind(&referrer.id)
    .bind(&referrer.slot)
    .bind(id.as_str())
    .execute(conn)
    .await
    .map_err(SqliteStorageError::Sqlx)?;
    Ok(())
}

pub struct SqliteMediaRepo {
    db: SqlitePools,
    root: PathBuf,
    files: MediaFiles,
}

impl SqliteMediaRepo {
    pub fn new(db: SqlitePools, root: PathBuf) -> Self {
        let files = MediaFiles::new(db.clone(), root.clone());
        Self { db, root, files }
    }

    pub(crate) fn files(&self) -> MediaFiles {
        self.files.clone()
    }

    pub async fn sweep_stale_incoming(&self) -> Result<usize, StorageError> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || remove_entries(&root, is_stale_incoming))
            .await
            .map_err(task_failure)?
            .map_err(StorageError::Io)
    }

    pub async fn sweep_unindexed_blobs(&self) -> Result<usize, StorageError> {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT id, format FROM media_blobs")
            .fetch_all(self.db.reader())
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        let indexed: HashSet<String> = rows
            .into_iter()
            .map(|(id, format)| format!("{id}{BLOB_FILE_EXTENSION_SEPARATOR}{format}"))
            .collect();
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            remove_entries(&root, |entry, now| {
                is_stale_unindexed_blob(entry, &indexed, now)
            })
        })
        .await
        .map_err(task_failure)?
        .map_err(StorageError::Io)
    }

    async fn begin_write(&self) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, StorageError> {
        Ok(self
            .db
            .writer()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(SqliteStorageError::Sqlx)?)
    }

    async fn index(
        &self,
        id: &MediaBlobId,
        format: MediaFormat,
        byte_size: u64,
        label: &str,
    ) -> Result<MediaBlob, StorageError> {
        let mut tx = self.begin_write().await?;
        sqlx::query(concat!(
            "INSERT INTO media_blobs (",
            media_blob_columns!(),
            ")
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO NOTHING"
        ))
        .bind(id.as_str())
        .bind(format.as_str())
        .bind(i64::try_from(byte_size).unwrap_or(i64::MAX))
        .bind(label)
        .bind(to_epoch_ms(OffsetDateTime::now_utc()))
        .execute(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        if format.kind() == MediaKind::Image {
            hold_in_library(&mut tx, id).await?;
        }
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;

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

        let (id, bytes) = tokio::task::spawn_blocking(move || {
            write_blob(&root, format, &bytes).map(|id| (id, bytes))
        })
        .await
        .map_err(task_failure)??;

        let blob = self.index(&id, format, byte_size, &accepted.label).await?;

        let root = self.root.clone();
        tokio::task::spawn_blocking(move || place_blob(&root, &id, format, &bytes))
            .await
            .map_err(task_failure)??;
        Ok(blob)
    }

    async fn import_file(&self, source: &Path) -> Result<MediaBlob, StorageError> {
        let source = source.to_path_buf();
        let (label, bytes) = tokio::task::spawn_blocking(move || read_import(&source))
            .await
            .map_err(task_failure)??;

        self.store(&label, bytes).await
    }

    async fn get(&self, id: &MediaBlobId) -> Result<Option<MediaBlob>, StorageError> {
        let row: Option<MediaBlobRow> = sqlx::query_as(concat!(
            "SELECT ",
            media_blob_columns!(),
            " FROM media_blobs WHERE id = ?"
        ))
        .bind(id.as_str())
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|r| decode_row(r).map_err(StorageError::from))
            .transpose()
    }

    async fn list(&self) -> Result<Vec<MediaBlob>, StorageError> {
        let rows: Vec<MediaBlobRow> = sqlx::query_as(concat!(
            "SELECT ",
            media_blob_columns!(),
            "
             FROM media_blobs
             ORDER BY imported_at DESC, label ASC"
        ))
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|row| decode_row(row).map_err(StorageError::from))
            .collect()
    }

    async fn total_bytes(&self) -> Result<u64, StorageError> {
        let (total,): (i64,) =
            sqlx::query_as("SELECT COALESCE(SUM(byte_size), 0) FROM media_blobs")
                .fetch_one(self.db.reader())
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
        let mut tx = self.begin_write().await?;
        let (holders,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM media_references WHERE blob_id = ? AND referrer_kind <> ?",
        )
        .bind(id.as_str())
        .bind(MediaReferrerKind::Library.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        if holders > 0 {
            return Err(StorageError::MediaReferenced {
                referrer_count: u32::try_from(holders).unwrap_or(u32::MAX),
            });
        }

        let mut released = ReleasedBlobs::default();
        release_all_references(
            &mut tx,
            MediaReferrerKind::Library,
            id.as_str(),
            &mut released,
        )
        .await?;
        released.note(id.as_str().to_owned());
        let present: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM media_blobs WHERE id = ?")
            .bind(id.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        if present.is_none() {
            return Ok(false);
        }

        self.files.commit(tx, released).await?;
        Ok(true)
    }

    async fn retain(&self, referrer: &MediaReferrer, id: &MediaBlobId) -> Result<(), StorageError> {
        let mut tx = self.begin_write().await?;
        let mut released = ReleasedBlobs::default();
        retain_reference(&mut tx, referrer, id, &mut released).await?;
        self.files.commit(tx, released).await
    }

    async fn release(&self, referrer: &MediaReferrer) -> Result<bool, StorageError> {
        let mut tx = self.begin_write().await?;
        let mut released = ReleasedBlobs::default();
        let dropped = release_reference(&mut tx, referrer, &mut released).await?;
        self.files.commit(tx, released).await?;
        Ok(dropped)
    }

    async fn release_all(
        &self,
        kind: MediaReferrerKind,
        referrer_id: &str,
    ) -> Result<u64, StorageError> {
        let mut tx = self.begin_write().await?;
        let mut released = ReleasedBlobs::default();
        let dropped = release_all_references(&mut tx, kind, referrer_id, &mut released).await?;
        self.files.commit(tx, released).await?;
        Ok(dropped)
    }

    async fn referrers(&self, id: &MediaBlobId) -> Result<Vec<MediaReferrer>, StorageError> {
        let rows: Vec<MediaReferenceRow> = sqlx::query_as(
            "SELECT referrer_kind, referrer_id, slot
             FROM media_references
             WHERE blob_id = ? AND referrer_kind <> ?
             ORDER BY referrer_kind ASC, referrer_id ASC, slot ASC",
        )
        .bind(id.as_str())
        .bind(MediaReferrerKind::Library.as_str())
        .fetch_all(self.db.reader())
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
        .fetch_optional(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        row.map(|(blob_id,)| MediaBlobId::from_stored(blob_id))
            .transpose()
    }

    async fn blobs_by_referrer_id(
        &self,
        kind: MediaReferrerKind,
        slot: &str,
    ) -> Result<HashMap<String, MediaBlobId>, StorageError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT referrer_id, blob_id FROM media_references
             WHERE referrer_kind = ? AND slot = ?",
        )
        .bind(kind.as_str())
        .bind(slot)
        .fetch_all(self.db.reader())
        .await
        .map_err(SqliteStorageError::Sqlx)?;

        rows.into_iter()
            .map(|(referrer_id, blob_id)| Ok((referrer_id, MediaBlobId::from_stored(blob_id)?)))
            .collect()
    }
}
