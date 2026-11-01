use std::io;
use std::path::{Path, PathBuf};

use forge_storage::{DataProvider, MediaBlobId};

use crate::bundle::naming::unique_suffix;
use crate::documents::overlay_files::{PENDING_IMPORT_MARKER, UNCOMMITTED_IMPORT_PREFIX};

pub(crate) enum Undo {
    Media(MediaBlobId),
    OverlayDir(PathBuf),
    OverlayFile {
        path: PathBuf,
        set_aside: Option<PathBuf>,
    },
}

pub(crate) struct UndoJournal {
    entries: Vec<Undo>,
    session: String,
    uncommitted_marker: Option<PathBuf>,
}

impl Default for UndoJournal {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            session: unique_suffix(),
            uncommitted_marker: None,
        }
    }
}

impl UndoJournal {
    pub(crate) fn session(&self) -> &str {
        &self.session
    }

    pub(crate) async fn mark_uncommitted(&mut self, overlay_root: &Path) -> io::Result<()> {
        if self.uncommitted_marker.is_some() {
            return Ok(());
        }
        let marker = overlay_root.join(format!("{UNCOMMITTED_IMPORT_PREFIX}{}", self.session));
        tokio::fs::write(&marker, []).await?;
        self.uncommitted_marker = Some(marker);
        Ok(())
    }

    pub(crate) async fn commit(&mut self) {
        let Some(marker) = self.uncommitted_marker.take() else {
            return;
        };
        match tokio::fs::remove_file(&marker).await {
            Ok(()) => self.discard_set_aside_files().await,
            Err(error) => {
                tracing::warn!(
                    %error,
                    path = %marker.display(),
                    "overlay import stays uncommitted on disk until the next start"
                );
                self.uncommitted_marker = Some(marker);
            }
        }
    }

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

    async fn discard_set_aside_files(&self) {
        for entry in &self.entries {
            let Undo::OverlayFile {
                set_aside: Some(set_aside),
                ..
            } = entry
            else {
                continue;
            };
            if let Err(error) = tokio::fs::remove_file(set_aside).await {
                tracing::warn!(
                    %error,
                    path = %set_aside.display(),
                    "replaced overlay file could not be cleaned up"
                );
            }
        }
    }

    pub(crate) async fn roll_back(
        mut self,
        provider: &dyn DataProvider,
    ) -> Result<(), RollbackFailures> {
        let mut failures = Vec::new();
        for undo in std::mem::take(&mut self.entries).into_iter().rev() {
            if let Err(e) = revert(provider, undo).await {
                failures.push(e);
            }
        }
        if failures.is_empty() {
            if let Some(marker) = &self.uncommitted_marker
                && let Err(e) = tokio::fs::remove_file(marker).await
                && e.kind() != io::ErrorKind::NotFound
            {
                return Err(RollbackFailures(vec![UndoFailure::File(e)]));
            }
            Ok(())
        } else {
            Err(RollbackFailures(failures))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UndoFailure {
    #[error(transparent)]
    Storage(#[from] forge_storage::StorageError),

    #[error(transparent)]
    File(#[from] io::Error),
}

#[derive(Debug, Default)]
pub struct RollbackFailures(Vec<UndoFailure>);

impl std::error::Error for RollbackFailures {}

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
        Undo::OverlayFile { path, set_aside } => match set_aside {
            Some(set_aside) => Ok(tokio::fs::rename(&set_aside, &path).await?),
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
