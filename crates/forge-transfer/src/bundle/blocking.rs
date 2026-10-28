use crate::error::TransferError;

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
