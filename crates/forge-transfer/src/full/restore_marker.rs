use std::path::{Component, Path, PathBuf};

use forge_update::update_in_flight;
use serde::{Deserialize, Serialize};

use crate::bundle::blocking::run_blocking;
use crate::bundle::durable_fs::{
    exists, read_record, remove_dir_all_if_present, remove_record, write_record,
};
use crate::error::TransferError;
use crate::full::stage::{StagedRestore, restore_staging_root};

pub const RESTORE_MARKER_FILE: &str = "restore-pending.json";
const RESTORE_MARKER_FORMAT: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateActivity {
    Idle,
    Staged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RestoreMarker {
    format: u32,
    staging: String,
}

impl RestoreMarker {
    fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(RESTORE_MARKER_FILE)
    }

    pub(crate) fn read(data_dir: &Path) -> Result<Option<Self>, TransferError> {
        let marker = read_record::<Self>(&Self::path_in(data_dir))?;
        match marker {
            Some(marker) if marker.format != RESTORE_MARKER_FORMAT => {
                Err(TransferError::MalformedRecord {
                    reason: format!("unsupported restore marker format {}", marker.format),
                })
            }
            other => Ok(other),
        }
    }

    pub(crate) fn clear(data_dir: &Path) -> Result<(), TransferError> {
        remove_record(&Self::path_in(data_dir))
    }

    pub(crate) fn staging_dir(&self, data_dir: &Path) -> Result<PathBuf, TransferError> {
        let mut components = Path::new(&self.staging).components();
        match (components.next(), components.next()) {
            (Some(Component::Normal(name)), None) => Ok(restore_staging_root(data_dir).join(name)),
            _ => Err(TransferError::ForeignStaging),
        }
    }
}

pub fn restore_pending(data_dir: &Path) -> Result<bool, TransferError> {
    Ok(exists(&RestoreMarker::path_in(data_dir))?)
}

pub async fn record_pending_restore(
    data_dir: &Path,
    staged: &StagedRestore,
    update: UpdateActivity,
) -> Result<(), TransferError> {
    if update == UpdateActivity::Staged {
        return Err(TransferError::UpdateInProgress);
    }
    let staging_root = restore_staging_root(data_dir);
    let staging = staged
        .staging_dir()
        .strip_prefix(&staging_root)
        .ok()
        .and_then(|relative| relative.to_str())
        .filter(|name| !name.is_empty())
        .ok_or(TransferError::ForeignStaging)?
        .to_owned();
    let marker = RestoreMarker {
        format: RESTORE_MARKER_FORMAT,
        staging,
    };
    marker.staging_dir(data_dir)?;
    run_blocking(data_dir.to_path_buf(), move |data_dir| {
        if update_in_flight(&data_dir)? {
            return Err(TransferError::UpdateInProgress);
        }
        if restore_pending(&data_dir)? {
            return Err(TransferError::RestoreAlreadyPending);
        }
        write_record(&RestoreMarker::path_in(&data_dir), &marker)
    })
    .await
}

pub async fn cancel_pending_restore(data_dir: &Path) -> Result<(), TransferError> {
    run_blocking(data_dir.to_path_buf(), |data_dir| {
        let staging_dir = match RestoreMarker::read(&data_dir) {
            Ok(Some(marker)) => marker.staging_dir(&data_dir).ok(),
            Ok(None) | Err(TransferError::MalformedRecord { .. }) => None,
            Err(e) => return Err(e),
        };
        RestoreMarker::clear(&data_dir)?;
        match staging_dir {
            Some(dir) => Ok(remove_dir_all_if_present(&dir)?),
            None => Ok(()),
        }
    })
    .await
}
