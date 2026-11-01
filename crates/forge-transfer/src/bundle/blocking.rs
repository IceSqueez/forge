use std::path::{Path, PathBuf};

use crate::bundle::durable_fs::remove_dir_all_if_present;
use crate::error::TransferError;

pub(crate) async fn run_blocking<A, T, F>(argument: A, work: F) -> Result<T, TransferError>
where
    A: Send + 'static,
    T: Send + 'static,
    F: FnOnce(A) -> Result<T, TransferError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || work(argument))
        .await
        .map_err(|e| {
            let reason = e.to_string();
            if e.is_panic() {
                TransferError::WorkerFailed { reason }
            } else {
                TransferError::Interrupted { reason }
            }
        })?
}

pub(crate) async fn stage_into<A, T, F>(
    staging_dir: &Path,
    argument: A,
    work: F,
) -> Result<T, TransferError>
where
    A: Send + 'static,
    T: Send + 'static,
    F: FnOnce(A, &Path) -> Result<T, TransferError> + Send + 'static,
{
    let target = staging_dir.to_path_buf();
    let staged = run_blocking(argument, move |argument| work(argument, &target)).await;
    if staged.is_err()
        && let Err(error) = discard_staging(staging_dir.to_path_buf()).await
    {
        tracing::warn!(%error, path = %staging_dir.display(), "staging could not be removed");
    }
    staged
}

pub(crate) async fn discard_staging(staging_dir: PathBuf) -> Result<(), TransferError> {
    run_blocking(staging_dir, |staging_dir| {
        Ok(remove_dir_all_if_present(&staging_dir)?)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panicking_worker_surfaces_as_worker_failed_carrying_the_panic_message() {
        let result: Result<(), _> = run_blocking((), |()| -> Result<(), TransferError> {
            panic!("disk exploded")
        })
        .await;

        assert!(
            matches!(&result, Err(TransferError::WorkerFailed { reason }) if reason.contains("panic")),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn a_returning_worker_passes_its_value_and_its_error_through_unchanged() {
        let ok = run_blocking(2_u32, |n| Ok(n * 3)).await;
        let err: Result<u32, _> = run_blocking((), |()| Err(TransferError::PackPlanStale)).await;

        assert!(matches!(ok, Ok(6)), "{ok:?}");
        assert!(matches!(err, Err(TransferError::PackPlanStale)), "{err:?}");
    }
}
