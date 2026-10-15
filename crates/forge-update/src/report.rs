use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::UpdateError;
use crate::apply_state::{ApplyPhase, ApplyState};
use crate::health::same_version;
use crate::journal::UpdateJournal;
use crate::record::{read_record, remove_record, require_format, write_record};
use crate::supervisor_lock::SupervisorLock;

pub const OUTCOME_FILE: &str = "update-outcome.json";
pub const OUTCOME_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateReport {
    Updated {
        previous_version: String,
        version: String,
    },
    RolledBack {
        failed_version: String,
        restored_version: String,
        reason: RollbackReason,
        database: DatabaseRestore,
    },
    RollbackFailed {
        failed_version: String,
        reason: RollbackReason,
        detail: String,
    },
    NotApplied {
        version: String,
        reason: ApplyFailure,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RollbackReason {
    Crashed { exit_code: Option<i32> },
    NoHealthSignal,
    LaunchFailed { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApplyFailure {
    ParentStillRunning,
    InstallerFailed { exit_code: Option<i32> },
    InstallerTimedOut,
    MalformedPayload,
    FileOperation { detail: String },
    Interrupted,
}

impl ApplyFailure {
    pub fn from_error(error: &UpdateError) -> Self {
        match error {
            UpdateError::ParentStillRunning => Self::ParentStillRunning,
            UpdateError::InstallerFailed(code) => Self::InstallerFailed {
                exit_code: Some(*code),
            },
            UpdateError::InstallerTerminated => Self::InstallerFailed { exit_code: None },
            UpdateError::InstallerTimedOut => Self::InstallerTimedOut,
            UpdateError::MalformedPayload => Self::MalformedPayload,
            other => Self::FileOperation {
                detail: other.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DatabaseRestore {
    Unchanged,
    Restored,
    Failed { detail: String },
}

#[derive(Debug, Serialize, Deserialize)]
struct OutcomeRecord {
    format: u32,
    report: UpdateReport,
}

fn outcome_path(data_dir: &Path) -> PathBuf {
    data_dir.join(OUTCOME_FILE)
}

pub(crate) fn record_outcome(data_dir: &Path, report: UpdateReport) -> Result<(), UpdateError> {
    write_record(
        &outcome_path(data_dir),
        &OutcomeRecord {
            format: OUTCOME_FORMAT,
            report,
        },
    )
}

pub fn take_update_report(
    data_dir: &Path,
    running_version: &str,
) -> Result<Option<UpdateReport>, UpdateError> {
    let path = outcome_path(data_dir);
    let record = match read_record::<OutcomeRecord>(&path) {
        Ok(record) => record,
        Err(error @ UpdateError::MalformedRecord(_)) => {
            remove_record(&path)?;
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    if let Some(record) = record {
        remove_record(&path)?;
        require_format(record.format, OUTCOME_FORMAT)?;
        return Ok(Some(record.report));
    }
    candidate_report(data_dir, running_version)
}

fn candidate_report(
    data_dir: &Path,
    running_version: &str,
) -> Result<Option<UpdateReport>, UpdateError> {
    let awaiting_health =
        ApplyState::read(data_dir)?.is_some_and(|state| state.phase == ApplyPhase::AwaitingHealth);
    if !awaiting_health || !SupervisorLock::is_held(data_dir)? {
        return Ok(None);
    }
    Ok(UpdateJournal::read_blocking(data_dir)?
        .filter(|journal| same_version(&journal.to_version, running_version))
        .map(|journal| UpdateReport::Updated {
            previous_version: journal.from_version,
            version: journal.to_version,
        }))
}
