use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::UpdateError;
use crate::download::StagedArtifact;
use crate::install::{InstallLayout, UpdatableInstall};
use crate::journal::{JOURNAL_FORMAT, UpdateJournal};
use crate::record::{read_record, remove_record, require_format, write_record};

const APPLY_STATE_FILE: &str = "update-apply.json";
const APPLY_STATE_FORMAT: u32 = 1;
pub const PREVIOUS_BUILD_PREFIX: &str = ".forge-previous-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyMode {
    Restart,
    OnQuit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApplyPhase {
    HandedOff,
    Applying,
    AwaitingFirstStart,
    AwaitingHealth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ApplyState {
    pub(crate) format: u32,
    pub(crate) mode: ApplyMode,
    pub(crate) layout: InstallLayout,
    pub(crate) staged_artifact: PathBuf,
    pub(crate) launch_executable: PathBuf,
    pub(crate) phase: ApplyPhase,
}

impl ApplyState {
    pub(crate) fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(APPLY_STATE_FILE)
    }

    pub(crate) fn read(data_dir: &Path) -> Result<Option<Self>, UpdateError> {
        let Some(state) = read_record::<Self>(&Self::path_in(data_dir))? else {
            return Ok(None);
        };
        require_format(state.format, APPLY_STATE_FORMAT)?;
        Ok(Some(state))
    }

    pub(crate) fn write(&self, data_dir: &Path) -> Result<(), UpdateError> {
        write_record(&Self::path_in(data_dir), self)
    }

    pub(crate) fn clear(data_dir: &Path) -> Result<(), UpdateError> {
        remove_record(&Self::path_in(data_dir))
    }
}

#[derive(Debug, Clone)]
pub struct HandoffRequest<'a> {
    pub install: &'a UpdatableInstall,
    pub staged: &'a StagedArtifact,
    pub from_version: &'a str,
    pub to_version: &'a str,
    pub database_snapshot: PathBuf,
    pub schema_version: u32,
    pub running_executable: &'a Path,
    pub mode: ApplyMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffPlan {
    journal: UpdateJournal,
    state: ApplyState,
}

impl HandoffPlan {
    pub fn new(request: HandoffRequest<'_>) -> Result<Self, UpdateError> {
        let target = request.install.target.clone();
        let journal = UpdateJournal {
            format: JOURNAL_FORMAT,
            from_version: request.from_version.to_owned(),
            to_version: request.to_version.to_owned(),
            previous_build: previous_build_path(&target)?,
            install_target: target.clone(),
            database_snapshot: request.database_snapshot,
            schema_version: request.schema_version,
        };
        let state = ApplyState {
            format: APPLY_STATE_FORMAT,
            mode: request.mode,
            layout: request.install.layout,
            staged_artifact: request.staged.path.clone(),
            launch_executable: launch_executable(
                request.install.layout,
                &target,
                request.running_executable,
            ),
            phase: ApplyPhase::HandedOff,
        };
        Ok(Self { journal, state })
    }

    pub fn write(&self, data_dir: &Path) -> Result<(), UpdateError> {
        self.journal.write_blocking(data_dir)?;
        self.state.write(data_dir)
    }
}

pub fn previous_build_path(target: &Path) -> Result<PathBuf, UpdateError> {
    prefixed_sibling(target, PREVIOUS_BUILD_PREFIX)
}

pub fn launch_executable(
    layout: InstallLayout,
    target: &Path,
    running_executable: &Path,
) -> PathBuf {
    match layout {
        InstallLayout::LinuxAppImage => target.to_path_buf(),
        InstallLayout::MacAppBundle
        | InstallLayout::WindowsPortable
        | InstallLayout::WindowsPerUserMsi => running_executable.to_path_buf(),
    }
}

pub(crate) fn prefixed_sibling(path: &Path, prefix: &str) -> Result<PathBuf, UpdateError> {
    let name = path.file_name().ok_or(UpdateError::PathWithoutFileName)?;
    let mut sibling = OsString::from(prefix);
    sibling.push(name);
    Ok(path.with_file_name(sibling))
}
