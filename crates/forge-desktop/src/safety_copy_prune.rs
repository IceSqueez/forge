use std::path::PathBuf;
use std::sync::Arc;

use forge_storage::DataProvider;
use forge_transfer::{SafetyCopyReport, TransferError, TransferExclusivity, prune_safety_copies};

pub fn spawn_boot_prune(
    backend: Arc<dyn DataProvider>,
    data_dir: PathBuf,
    exclusivity: TransferExclusivity,
) {
    let mut held = exclusivity.watch_held();
    tokio::spawn(async move {
        loop {
            match prune_safety_copies(backend.as_ref(), &data_dir, &exclusivity).await {
                Ok(report) if report.deferred_by_transfer => {
                    tracing::info!("safety copy pruning waits for the running transfer");
                    if held.changed().await.is_none() {
                        return;
                    }
                }
                outcome => {
                    log_outcome(outcome);
                    return;
                }
            }
        }
    });
}

fn log_outcome(outcome: Result<SafetyCopyReport, TransferError>) {
    match outcome {
        Ok(report) => {
            for failure in &report.failures {
                tracing::warn!(
                    path = %failure.path.display(),
                    reason = %failure.reason,
                    "could not delete a safety copy"
                );
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not prune safety copies"),
    }
}
