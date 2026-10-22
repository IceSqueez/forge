use std::path::PathBuf;
use std::sync::Arc;

use forge_storage::DataProvider;
use forge_transfer::{TransferExclusivity, prune_safety_copies};

pub fn spawn_boot_prune(
    backend: Arc<dyn DataProvider>,
    data_dir: PathBuf,
    exclusivity: TransferExclusivity,
) {
    tokio::spawn(async move {
        match prune_safety_copies(backend.as_ref(), &data_dir, &exclusivity).await {
            Ok(report) => {
                if report.deferred_by_transfer {
                    tracing::info!("safety copy pruning deferred because a transfer is running");
                }
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
    });
}
