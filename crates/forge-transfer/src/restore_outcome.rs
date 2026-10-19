use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreOutcome {
    NonePending,
    Applied { pre_restore_snapshot: PathBuf },
    RolledBack { reason: RestoreRollback },
    Deferred { reason: RestoreDeferral },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RestoreRollback {
    Interrupted,
    StagingMissing,
    MarkerUnreadable,
    Failed { error: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreDeferral {
    UpdateInProgress,
    InstanceLockUnavailable,
}
