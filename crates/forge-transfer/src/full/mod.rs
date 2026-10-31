mod restore_apply;
pub(crate) mod restore_journal;
mod restore_marker;
mod restore_outcome;
pub(crate) mod snapshot;
mod stage;
pub(crate) mod writer;

pub use restore_apply::apply_pending_restore;
pub use restore_journal::{RESTORE_JOURNAL_FILE, restore_in_flight};
pub use restore_marker::{
    RESTORE_MARKER_FILE, UpdateActivity, cancel_pending_restore, record_pending_restore,
    restore_pending,
};
pub use restore_outcome::{RestoreDeferral, RestoreOutcome, RestoreRollback};
pub use stage::{
    RESTORE_STAGING_DIR_NAME, StageRequest, StagedRestore, discard_staged_restore,
    restore_staging_root, stage_full_restore,
};
pub use writer::{
    BackupDestination, FullBackupRequest, ScheduledBackupRequest, ScheduledWrite,
    write_full_backup, write_scheduled_backup, write_scheduled_backup_cancellable,
    write_scheduled_backup_unless_unchanged,
};
