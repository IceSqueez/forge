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
