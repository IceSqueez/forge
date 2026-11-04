use std::collections::BTreeSet;
use std::path::PathBuf;

use forge_storage::{MediaBlobId, MediaFormat, StorageError};
use sqlx::{Sqlite, SqliteConnection, Transaction};

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;
use crate::repos::media::{blob_path, task_failure};

#[derive(Debug)]
pub(crate) struct OrphanedBlob {
    id: MediaBlobId,
    format: MediaFormat,
}

fn orphan(id: String, format: &str) -> Option<OrphanedBlob> {
    let parsed = MediaBlobId::from_stored(id.as_str())
        .ok()
        .zip(MediaFormat::parse(format));
    if parsed.is_none() {
        tracing::warn!(blob = %id, format, "unreadable media row removed; its file is left to the unindexed sweep");
    }
    parsed.map(|(id, format)| OrphanedBlob { id, format })
}

#[derive(Debug, Default)]
pub(crate) struct ReleasedBlobs(BTreeSet<String>);

impl ReleasedBlobs {
    pub(crate) fn note(&mut self, blob_id: String) {
        self.0.insert(blob_id);
    }

    async fn collect(self, conn: &mut SqliteConnection) -> Result<Vec<OrphanedBlob>, StorageError> {
        let mut orphans = Vec::new();
        for blob_id in self.0 {
            let removed: Option<(String,)> = sqlx::query_as(
                "DELETE FROM media_blobs
                 WHERE id = ?
                   AND NOT EXISTS (SELECT 1 FROM media_references WHERE blob_id = media_blobs.id)
                 RETURNING format",
            )
            .bind(&blob_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
            if let Some((format,)) = removed {
                orphans.extend(orphan(blob_id, &format));
            }
        }
        Ok(orphans)
    }
}

pub(crate) async fn remove_unreferenced_blobs(
    db: &SqlitePools,
) -> Result<Vec<OrphanedBlob>, StorageError> {
    let removed: Vec<(String, String)> = sqlx::query_as(
        "DELETE FROM media_blobs
         WHERE NOT EXISTS (SELECT 1 FROM media_references WHERE blob_id = media_blobs.id)
         RETURNING id, format",
    )
    .fetch_all(db.writer())
    .await
    .map_err(SqliteStorageError::Sqlx)?;

    Ok(removed
        .into_iter()
        .filter_map(|(id, format)| orphan(id, &format))
        .collect())
}

#[derive(Clone)]
pub(crate) struct MediaFiles {
    db: SqlitePools,
    root: PathBuf,
}

impl MediaFiles {
    pub(crate) fn new(db: SqlitePools, root: PathBuf) -> Self {
        Self { db, root }
    }

    pub(crate) async fn commit(
        &self,
        mut tx: Transaction<'_, Sqlite>,
        released: ReleasedBlobs,
    ) -> Result<(), StorageError> {
        let orphans = released.collect(&mut tx).await?;
        tx.commit().await.map_err(SqliteStorageError::Sqlx)?;
        self.discard(orphans).await;
        Ok(())
    }

    pub(crate) async fn discard(&self, orphans: Vec<OrphanedBlob>) {
        for orphan in orphans {
            if let Err(error) = self.discard_file(&orphan).await {
                tracing::warn!(blob = %orphan.id, %error, "unreferenced media file left behind after its index row was removed");
            }
        }
    }

    async fn discard_file(&self, orphan: &OrphanedBlob) -> Result<(), StorageError> {
        let mut guard = self
            .db
            .writer()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        let revived: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM media_blobs WHERE id = ?")
            .bind(orphan.id.as_str())
            .fetch_optional(&mut *guard)
            .await
            .map_err(SqliteStorageError::Sqlx)?;
        if revived.is_some() {
            return Ok(());
        }

        let path = blob_path(&self.root, &orphan.id, orphan.format);
        let removed = tokio::task::spawn_blocking(move || std::fs::remove_file(path))
            .await
            .map_err(task_failure)?;
        guard.rollback().await.map_err(SqliteStorageError::Sqlx)?;

        match removed {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StorageError::Io(error)),
        }
    }
}
