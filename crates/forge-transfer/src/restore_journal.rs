use std::path::{Path, PathBuf};

use forge_storage::paths::CREDENTIALS_KEY_FILE_NAME;
use forge_storage::{DATABASE_FILE_NAME, database_unit_file_names};
use serde::{Deserialize, Serialize};

use crate::durable_fs::{exists, read_record, remove_record, write_record};
use crate::error::TransferError;
use crate::layout::{ARCHIVE_ROOT, INCLUDED_DIRS};
use crate::restore_outcome::RestoreRollback;

pub const RESTORE_JOURNAL_FILE: &str = "restore-journal.json";
const RESTORE_JOURNAL_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "phase")]
pub(crate) enum RestorePhase {
    SnapshottingCurrent,
    PlacingStaged,
    Committed,
    RollingBack { reason: RestoreRollback },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RestoreJournal {
    pub(crate) format: u32,
    pub(crate) phase: RestorePhase,
    pub(crate) staging_dir: PathBuf,
    pub(crate) snapshot_dir: PathBuf,
    pub(crate) credentials_key: PathBuf,
    pub(crate) original_key: bool,
    pub(crate) staged_entries: Vec<String>,
    pub(crate) staged_key: bool,
}

impl RestoreJournal {
    fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(RESTORE_JOURNAL_FILE)
    }

    pub(crate) fn begin(
        staging_dir: PathBuf,
        snapshot_dir: PathBuf,
        credentials_key: PathBuf,
    ) -> Result<Self, TransferError> {
        let payload = staging_dir.join(ARCHIVE_ROOT);
        let mut staged_entries = Vec::new();
        for name in std::iter::once(DATABASE_FILE_NAME).chain(INCLUDED_DIRS) {
            if exists(&payload.join(name))? {
                staged_entries.push(name.to_owned());
            }
        }
        Ok(Self {
            format: RESTORE_JOURNAL_FORMAT,
            phase: RestorePhase::SnapshottingCurrent,
            original_key: credentials_key.is_file(),
            staged_key: payload.join(CREDENTIALS_KEY_FILE_NAME).is_file(),
            staging_dir,
            snapshot_dir,
            credentials_key,
            staged_entries,
        })
    }

    pub(crate) fn read(data_dir: &Path) -> Result<Option<Self>, TransferError> {
        let journal = read_record::<Self>(&Self::path_in(data_dir))?;
        match journal {
            Some(journal) if journal.format != RESTORE_JOURNAL_FORMAT => {
                Err(TransferError::MalformedRecord {
                    reason: format!("unsupported restore journal format {}", journal.format),
                })
            }
            other => Ok(other),
        }
    }

    pub(crate) fn advance(
        &mut self,
        data_dir: &Path,
        phase: RestorePhase,
    ) -> Result<(), TransferError> {
        self.phase = phase;
        write_record(&Self::path_in(data_dir), self)
    }

    pub(crate) fn write(&self, data_dir: &Path) -> Result<(), TransferError> {
        write_record(&Self::path_in(data_dir), self)
    }

    pub(crate) fn clear(data_dir: &Path) -> Result<(), TransferError> {
        remove_record(&Self::path_in(data_dir))
    }

    pub(crate) fn payload_dir(&self) -> PathBuf {
        self.staging_dir.join(ARCHIVE_ROOT)
    }

    pub(crate) fn staged_key_file(&self) -> PathBuf {
        self.payload_dir().join(CREDENTIALS_KEY_FILE_NAME)
    }

    pub(crate) fn snapshot_key_file(&self) -> PathBuf {
        self.snapshot_dir.join(CREDENTIALS_KEY_FILE_NAME)
    }
}

pub fn restore_in_flight(data_dir: &Path) -> Result<bool, TransferError> {
    Ok(exists(&RestoreJournal::path_in(data_dir))?)
}

pub(crate) fn backup_scope_entries() -> Vec<String> {
    database_unit_file_names()
        .into_iter()
        .chain(INCLUDED_DIRS.iter().map(|dir| (*dir).to_owned()))
        .collect()
}
