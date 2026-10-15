use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::UpdateError;
use crate::atomic_file::{write_atomically, write_atomically_blocking};
use crate::record::remove_record;

pub const JOURNAL_FILE: &str = "update-journal.json";
pub const JOURNAL_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateJournal {
    pub format: u32,
    pub from_version: String,
    pub to_version: String,
    pub install_target: PathBuf,
    pub previous_build: PathBuf,
    pub database_snapshot: PathBuf,
    pub schema_version: u32,
}

impl UpdateJournal {
    pub fn path_in(data_dir: &Path) -> PathBuf {
        data_dir.join(JOURNAL_FILE)
    }

    pub async fn write(&self, data_dir: &Path) -> Result<(), UpdateError> {
        write_atomically(&Self::path_in(data_dir), self.encode()?).await
    }

    pub async fn read(data_dir: &Path) -> Result<Option<Self>, UpdateError> {
        match tokio::fs::read(Self::path_in(data_dir)).await {
            Ok(bytes) => Self::decode(&bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn write_blocking(&self, data_dir: &Path) -> Result<(), UpdateError> {
        write_atomically_blocking(&Self::path_in(data_dir), &self.encode()?)
    }

    pub fn read_blocking(data_dir: &Path) -> Result<Option<Self>, UpdateError> {
        match std::fs::read(Self::path_in(data_dir)) {
            Ok(bytes) => Self::decode(&bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn clear_blocking(data_dir: &Path) -> Result<(), UpdateError> {
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

    pub async fn clear(data_dir: &Path) -> Result<(), UpdateError> {
        match tokio::fs::remove_file(Self::path_in(data_dir)).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
