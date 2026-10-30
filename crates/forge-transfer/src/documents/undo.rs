use std::io;
use std::path::PathBuf;

use forge_storage::{DataProvider, MediaBlobId};

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

    pub(crate) async fn roll_back(self, provider: &dyn DataProvider) -> Result<(), String> {
        let mut failures = Vec::new();
        for undo in self.entries.into_iter().rev() {
            if let Err(e) = revert(provider, undo).await {
                failures.push(e);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

async fn revert(provider: &dyn DataProvider, undo: Undo) -> Result<(), String> {
    let file = |e: io::Error| e.to_string();
    match undo {
        Undo::Media(id) => provider
            .media_repo()
            .delete(&id)
            .await
            .map(drop)
            .map_err(|e| e.to_string()),
        Undo::OverlayDir(path) => match tokio::fs::remove_dir_all(&path).await {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(file(e)),
            _ => Ok(()),
        },
        Undo::OverlayFile { path, previous } => match previous {
            Some(bytes) => tokio::fs::write(&path, bytes).await.map_err(file),
            None => match tokio::fs::remove_file(&path).await {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(file(e)),
                _ => Ok(()),
            },
        },
    }
}
