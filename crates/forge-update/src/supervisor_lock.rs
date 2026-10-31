use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use crate::UpdateError;

pub(crate) const SUPERVISOR_LOCK_FILE: &str = "update-supervisor.lock";

#[derive(Debug)]
pub(crate) struct SupervisorLock {
    _file: File,
}

impl SupervisorLock {
    fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(SUPERVISOR_LOCK_FILE)
    }

    pub(crate) fn try_acquire(data_dir: &Path) -> Result<Option<Self>, UpdateError> {
        std::fs::create_dir_all(data_dir)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(Self::path_in(data_dir))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }

    pub(crate) fn is_held(data_dir: &Path) -> Result<bool, UpdateError> {
        if !Self::path_in(data_dir).exists() {
            return Ok(false);
        }
        Ok(Self::try_acquire(data_dir)?.is_none())
    }
}
