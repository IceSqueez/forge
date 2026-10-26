use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_storage::DataProvider;

use crate::error::TransferError;
use crate::naming::{backups_dir, snapshot_path};

pub(crate) async fn with_database_snapshot<T, F>(
    provider: &Arc<dyn DataProvider>,
    data_dir: &Path,
    stem: &str,
    work: F,
) -> Result<T, TransferError>
where
    T: Send + 'static,
    F: FnOnce(PathBuf) -> Result<T, TransferError> + Send + 'static,
{
    tokio::fs::create_dir_all(backups_dir(data_dir)).await?;
    let snapshot = snapshot_path(data_dir, stem);
    let result = match provider.export(&snapshot).await {
        Ok(()) => run_blocking(snapshot.clone(), work).await,
        Err(e) => Err(TransferError::Storage(e)),
    };
    if let Err(error) = tokio::fs::remove_file(&snapshot).await {
        tracing::debug!(%error, path = %snapshot.display(), "partial snapshot could not be removed");
    }
    result
}

pub(crate) async fn run_blocking<A, T, F>(argument: A, work: F) -> Result<T, TransferError>
where
    A: Send + 'static,
    T: Send + 'static,
    F: FnOnce(A) -> Result<T, TransferError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || work(argument))
        .await
        .map_err(|e| TransferError::Interrupted {
            reason: e.to_string(),
        })?
}
