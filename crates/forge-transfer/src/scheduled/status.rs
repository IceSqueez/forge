use std::io::ErrorKind;

use time::OffsetDateTime;

use crate::error::TransferError;
use crate::scheduled::usage::StorageUsed;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    DestinationMissing,
    DestinationNotWritable,
    DiskFull,
    DestinationInsideBackupScope,
    KeyMissing,
    KeyUnreadable,
    SnapshotFailed,
    StateNotSaved,
    Interrupted,
}

impl FailureCategory {
    pub(crate) fn of(error: &TransferError) -> Self {
        match error {
            TransferError::Io(io) => match io.kind() {
                ErrorKind::StorageFull | ErrorKind::QuotaExceeded => Self::DiskFull,
                ErrorKind::NotFound => Self::DestinationMissing,
                ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem => {
                    Self::DestinationNotWritable
                }
                _ => Self::SnapshotFailed,
            },
            TransferError::Encryption { .. } => Self::KeyMissing,
            TransferError::Interrupted { .. } => Self::Interrupted,
            _ => Self::SnapshotFailed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailureStreak {
    pub category: FailureCategory,
    pub since: OffsetDateTime,
    pub attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleState {
    Disabled,
    Idle,
    Running,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledBackupStatus {
    pub state: ScheduleState,
    pub last_success_at: Option<OffsetDateTime>,
    pub failure: Option<FailureStreak>,
    pub next_due: Option<OffsetDateTime>,
    pub prune_failures: u32,
    pub storage_used: Option<StorageUsed>,
}

impl ScheduledBackupStatus {
    pub(crate) fn initial() -> Self {
        Self {
            state: ScheduleState::Disabled,
            last_success_at: None,
            failure: None,
            next_due: None,
            prune_failures: 0,
            storage_used: None,
        }
    }
}
