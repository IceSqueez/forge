use std::path::{Path, PathBuf};

use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::UpdateError;
use crate::digest::StreamingSha256;
use crate::http::UpdateClient;
use crate::manifest::{Artifact, is_plain_file_name};

pub const STAGING_PREFIX: &str = ".forge-update-";
const PARTIAL_SUFFIX: &str = ".part";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DownloadProgress {
    pub received: u64,
    pub total: u64,
}

#[derive(Debug)]
pub struct ProgressSender(watch::Sender<DownloadProgress>);

impl ProgressSender {
    fn publish(&self, progress: DownloadProgress) {
        self.0.send_replace(progress);
    }
}

#[derive(Debug, Clone)]
pub struct ProgressReceiver(watch::Receiver<DownloadProgress>);

impl ProgressReceiver {
    pub fn latest(&self) -> DownloadProgress {
        *self.0.borrow()
    }

    pub async fn changed(&mut self) -> Option<DownloadProgress> {
        self.0.changed().await.ok()?;
        Some(*self.0.borrow_and_update())
    }
}

pub fn progress_channel() -> (ProgressSender, ProgressReceiver) {
    let (tx, rx) = watch::channel(DownloadProgress::default());
    (ProgressSender(tx), ProgressReceiver(rx))
}

#[derive(Debug, Clone, Default)]
pub struct CancelSignal(CancellationToken);

impl CancelSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedArtifact {
    pub path: PathBuf,
    pub size: u64,
}

pub fn staged_path(staging_dir: &Path, artifact: &Artifact) -> PathBuf {
    staging_dir.join(format!("{STAGING_PREFIX}{}", artifact.name))
}

pub async fn download_artifact(
    client: &UpdateClient,
    artifact: &Artifact,
    staging_dir: &Path,
    progress: &ProgressSender,
    cancel: &CancelSignal,
) -> Result<StagedArtifact, UpdateError> {
    if !is_plain_file_name(&artifact.name) {
        return Err(UpdateError::InvalidArtifactName);
    }
    let final_path = staged_path(staging_dir, artifact);
    let partial_path =
        staging_dir.join(format!("{STAGING_PREFIX}{}{PARTIAL_SUFFIX}", artifact.name));
    let partial = PartialFile::new(partial_path);

    let mut response = tokio::select! {
        biased;
        () = cancel.0.cancelled() => return Err(UpdateError::Cancelled),
        response = client.open_stream(&artifact.url) => response?,
    };
    if response
        .content_length()
        .is_some_and(|length| length > artifact.size)
    {
        return Err(UpdateError::ArtifactOversized {
            expected: artifact.size,
        });
    }

    let mut file = File::create(partial.path()).await?;
    let mut hasher = StreamingSha256::default();
    let mut received: u64 = 0;
    progress.publish(DownloadProgress {
        received,
        total: artifact.size,
    });

    loop {
        let chunk = tokio::select! {
            biased;
            () = cancel.0.cancelled() => return Err(UpdateError::Cancelled),
            chunk = response.chunk() => chunk.map_err(UpdateError::network)?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        received = received.saturating_add(chunk.len() as u64);
        if received > artifact.size {
            return Err(UpdateError::ArtifactOversized {
                expected: artifact.size,
            });
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        progress.publish(DownloadProgress {
            received,
            total: artifact.size,
        });
    }

    if received != artifact.size {
        return Err(UpdateError::ArtifactSizeMismatch {
            expected: artifact.size,
            actual: received,
        });
    }
    if hasher.finish() != artifact.sha256 {
        return Err(UpdateError::ArtifactHashMismatch);
    }

    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::rename(partial.path(), &final_path).await?;
    partial.keep();

    Ok(StagedArtifact {
        path: final_path,
        size: received,
    })
}

struct PartialFile {
    path: PathBuf,
    armed: bool,
}

impl PartialFile {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn keep(mut self) {
        self.armed = false;
    }
}

impl Drop for PartialFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
