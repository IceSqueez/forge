use std::path::Path;

use crate::UpdateError;
use crate::apply::files::{path_exists, remove_path};
use crate::apply_state::{ApplyPhase, ApplyState};
use crate::health::{clear_health_marker, same_version};
use crate::journal::UpdateJournal;
use crate::report::{ApplyFailure, UpdateReport, record_outcome};
use crate::supervisor_lock::SupervisorLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartAction {
    Continue,
    SupervisorRunning,
    HandOff,
}

pub fn pending_start_action(
    data_dir: &Path,
    running_version: &str,
) -> Result<StartAction, UpdateError> {
    if SupervisorLock::is_held(data_dir)? {
        return Ok(StartAction::SupervisorRunning);
    }
    let (Some(journal), Some(mut state)) = (
        UpdateJournal::read_blocking(data_dir)?,
        ApplyState::read(data_dir)?,
    ) else {
        UpdateJournal::clear_blocking(data_dir)?;
        ApplyState::clear(data_dir)?;
        return Ok(StartAction::Continue);
    };
    if state.phase == ApplyPhase::AwaitingFirstStart {
        return Ok(StartAction::HandOff);
    }
    if same_version(&journal.to_version, running_version) {
        state.phase = ApplyPhase::AwaitingFirstStart;
        state.write(data_dir)?;
        return Ok(StartAction::HandOff);
    }
    abandon_interrupted_update(data_dir, &journal, &state)?;
    Ok(StartAction::Continue)
}

fn abandon_interrupted_update(
    data_dir: &Path,
    journal: &UpdateJournal,
    state: &ApplyState,
) -> Result<(), UpdateError> {
    if !path_exists(&journal.install_target) && path_exists(&journal.previous_build) {
        std::fs::rename(&journal.previous_build, &journal.install_target)?;
    }
    remove_path(&state.staged_artifact)?;
    record_outcome(
        data_dir,
        UpdateReport::NotApplied {
            version: journal.to_version.clone(),
            reason: ApplyFailure::Interrupted,
        },
    )?;
    clear_health_marker(data_dir)?;
    ApplyState::clear(data_dir)?;
    UpdateJournal::clear_blocking(data_dir)
}
