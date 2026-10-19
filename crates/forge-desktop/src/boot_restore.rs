use std::path::Path;

use forge_transfer::{
    RestoreDeferral, RestoreOutcome, TransferError, apply_pending_restore, restore_pending,
};
use gpui::Global;

pub(crate) struct BootRestore(#[expect(dead_code)] pub RestoreOutcome);

impl Global for BootRestore {}

pub(crate) fn settle_pending_restore(
    data_dir: &Path,
    lock_acquired: bool,
) -> Result<RestoreOutcome, TransferError> {
    let outcome = if lock_acquired {
        apply_pending_restore(data_dir)?
    } else if restore_pending(data_dir)? {
        RestoreOutcome::Deferred {
            reason: RestoreDeferral::InstanceLockUnavailable,
        }
    } else {
        RestoreOutcome::NonePending
    };
    match &outcome {
        RestoreOutcome::NonePending => {}
        RestoreOutcome::Applied {
            pre_restore_snapshot,
        } => tracing::info!(
            snapshot = %pre_restore_snapshot.display(),
            "pending restore applied"
        ),
        RestoreOutcome::RolledBack { reason } => {
            tracing::warn!(?reason, "pending restore rolled back")
        }
        RestoreOutcome::Deferred { reason } => {
            tracing::warn!(?reason, "restore is still pending and was not applied")
        }
    }
    Ok(outcome)
}
