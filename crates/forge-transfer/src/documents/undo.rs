use std::io;
use std::path::PathBuf;

use forge_storage::{DataProvider, MediaBlobId};

use crate::documents::overlay_files::PENDING_IMPORT_MARKER;

pub(crate) enum Undo {
    Media(MediaBlobId),
    OverlayDir(PathBuf),
    OverlayFile {
        path: PathBuf,
        previous: Option<Vec<u8>>,
    },
}

#[derive(Default)]
pub(crate) struct UndoJournal {
    entries: Vec<Undo>,
}

impl UndoJournal {
    pub(crate) fn push(&mut self, undo: Undo) {
        self.entries.push(undo);
    }

    pub(crate) async fn settle_overlay_dirs(&self) {
        for entry in &self.entries {
            let Undo::OverlayDir(directory) = entry else {
                continue;
            };
            let marker = directory.join(PENDING_IMPORT_MARKER);
            if let Err(error) = tokio::fs::remove_file(&marker).await {
                tracing::warn!(
                    %error,
                    path = %marker.display(),
                    "imported overlay stays marked until the next start"
                );
            }
        }
    }

    pub(crate) async fn roll_back(
        self,
        provider: &dyn DataProvider,
    ) -> Result<(), RollbackFailures> {
        let mut failures = Vec::new();
        for undo in self.entries.into_iter().rev() {
            if let Err(e) = revert(provider, undo).await {
                failures.push(e);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(RollbackFailures(failures))
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum UndoFailure {
    #[error(transparent)]
    Storage(#[from] forge_storage::StorageError),

    #[error(transparent)]
    File(#[from] io::Error),
}

#[derive(Debug)]
pub(crate) struct RollbackFailures(Vec<UndoFailure>);

impl std::fmt::Display for RollbackFailures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let failures: Vec<String> = self.0.iter().map(ToString::to_string).collect();
        f.write_str(&failures.join("; "))
    }
}

async fn revert(provider: &dyn DataProvider, undo: Undo) -> Result<(), UndoFailure> {
    match undo {
        Undo::Media(id) => provider
            .media_repo()
            .delete(&id)
            .await
            .map(drop)
            .map_err(UndoFailure::from),
        Undo::OverlayDir(path) => match tokio::fs::remove_dir_all(&path).await {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        },
        Undo::OverlayFile { path, previous } => match previous {
            Some(bytes) => Ok(tokio::fs::write(&path, bytes).await?),
            None => match tokio::fs::remove_file(&path).await {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
                _ => Ok(()),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::StorageError;

    use super::*;

    #[test]
    fn rollback_failures_read_as_one_line_joined_in_the_order_they_happened() {
        let failures = RollbackFailures(vec![
            UndoFailure::File(io::Error::other("overlay file is locked")),
            UndoFailure::Storage(StorageError::NotReady),
            UndoFailure::File(io::Error::other("overlay folder is locked")),
        ]);

        assert_eq!(
            failures.to_string(),
            "overlay file is locked; operation not ready: implementation pending; overlay folder is locked"
        );
    }

    #[test]
    fn a_single_rollback_failure_reads_without_a_separator() {
        let failures = RollbackFailures(vec![UndoFailure::Storage(StorageError::Decryption)]);

        assert_eq!(failures.to_string(), "decryption failed");
    }
}
