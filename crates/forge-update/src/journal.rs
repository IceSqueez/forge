use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::UpdateError;
use crate::record::{read_record, remove_record, require_format, write_record};

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
        write_record(&Self::path_in(data_dir), self)
    }

    pub(crate) fn read_blocking(data_dir: &Path) -> Result<Option<Self>, UpdateError> {
        let journal = read_record::<Self>(&Self::path_in(data_dir))?;
        if let Some(journal) = &journal {
            require_format(journal.format, JOURNAL_FORMAT)?;
        }
        Ok(journal)
    }

    pub(crate) fn clear_blocking(data_dir: &Path) -> Result<(), UpdateError> {
        remove_record(&Self::path_in(data_dir))
    }
}
