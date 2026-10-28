use std::path::{Path, PathBuf};
use std::sync::Arc;

use forge_storage::DataProvider;

use crate::bundle::blocking::run_blocking;
use crate::bundle::naming::{backups_dir, snapshot_path};
use crate::error::TransferError;

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
