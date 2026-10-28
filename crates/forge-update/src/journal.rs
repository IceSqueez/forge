use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::UpdateError;
use crate::atomic_file::write_atomically_blocking;
use crate::record::remove_record;

const JOURNAL_FILE: &str = "update-journal.json";
pub(crate) const JOURNAL_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UpdateJournal {
    pub(crate) format: u32,
    pub(crate) from_version: String,
    pub(crate) to_version: String,
    pub(crate) install_target: PathBuf,
    pub(crate) previous_build: PathBuf,
    pub(crate) database_snapshot: PathBuf,
    pub(crate) schema_version: u32,
}

impl UpdateJournal {
    pub(crate) fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(JOURNAL_FILE)
    }

    pub(crate) fn write_blocking(&self, data_dir: &Path) -> Result<(), UpdateError> {
        write_atomically_blocking(&Self::path_in(data_dir), &self.encode()?)
    }

    pub(crate) fn read_blocking(data_dir: &Path) -> Result<Option<Self>, UpdateError> {
        match std::fs::read(Self::path_in(data_dir)) {
            Ok(bytes) => Self::decode(&bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn clear_blocking(data_dir: &Path) -> Result<(), UpdateError> {
        remove_record(&Self::path_in(data_dir))
    }

    fn encode(&self) -> Result<Vec<u8>, UpdateError> {
        serde_json::to_vec_pretty(self)
            .map_err(|error| UpdateError::MalformedJournal(error.to_string()))
    }

    fn decode(bytes: &[u8]) -> Result<Self, UpdateError> {
        let journal: Self = serde_json::from_slice(bytes)
            .map_err(|error| UpdateError::MalformedJournal(error.to_string()))?;
        if journal.format != JOURNAL_FORMAT {
            return Err(UpdateError::MalformedJournal(format!(
                "unsupported journal format {}",
                journal.format
            )));
        }
        Ok(journal)
    }
}
