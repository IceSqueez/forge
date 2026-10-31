use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::UpdateError;
use crate::apply_state::{ApplyPhase, ApplyState};
use crate::record::{read_record, remove_record, write_record};
use crate::version::parse_version;

pub(crate) const HEALTH_MARKER_FILE: &str = "update-health.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HealthMarker {
    pub pid: u32,
    pub version: String,
}

impl HealthMarker {
    pub(crate) fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(HEALTH_MARKER_FILE)
    }

    pub(crate) fn is_for_version(&self, version: &str) -> bool {
        same_version(&self.version, version)
    }
}

pub fn write_health_marker(data_dir: &Path, running_version: &str) -> Result<bool, UpdateError> {
    let awaiting_health =
        ApplyState::read(data_dir)?.is_some_and(|state| state.phase == ApplyPhase::AwaitingHealth);
    if !awaiting_health {
        return Ok(false);
    }
    let marker = HealthMarker {
        pid: std::process::id(),
        version: running_version.to_owned(),
    };
    write_record(&HealthMarker::path_in(data_dir), &marker)?;
    Ok(true)
}

pub(crate) fn read_health_marker(data_dir: &Path) -> Result<Option<HealthMarker>, UpdateError> {
    read_record(&HealthMarker::path_in(data_dir))
}

pub(crate) fn clear_health_marker(data_dir: &Path) -> Result<(), UpdateError> {
    remove_record(&HealthMarker::path_in(data_dir))
}

pub(crate) fn same_version(left: &str, right: &str) -> bool {
    match (parse_version(left), parse_version(right)) {
        (Some(left), Some(right)) => left == right,
        _ => left.trim() == right.trim(),
    }
}
