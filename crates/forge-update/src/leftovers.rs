use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::UpdateError;
use crate::apply::files::remove_path;
use crate::record::{read_record, remove_record, write_record};

const LEFTOVERS_FILE: &str = "update-leftovers.json";
const LEFTOVERS_FORMAT: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct LeftoverRecord {
    format: u32,
    paths: Vec<PathBuf>,
}

fn record_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LEFTOVERS_FILE)
}

fn known_leftovers(data_dir: &Path) -> Vec<PathBuf> {
    read_record::<LeftoverRecord>(&record_path(data_dir))
        .ok()
        .flatten()
        .filter(|record| record.format == LEFTOVERS_FORMAT)
        .map_or_else(Vec::new, |record| record.paths)
}

fn write_leftovers(data_dir: &Path, paths: Vec<PathBuf>) -> Result<(), UpdateError> {
    write_record(
        &record_path(data_dir),
        &LeftoverRecord {
            format: LEFTOVERS_FORMAT,
            paths,
        },
    )
}

pub(crate) fn remember_leftovers(
    data_dir: &Path,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Result<(), UpdateError> {
    let mut known = known_leftovers(data_dir);
    known.extend(paths);
    write_leftovers(data_dir, known)
}

pub(crate) fn sweep_leftovers(data_dir: &Path) -> Result<(), UpdateError> {
    let mut remaining = known_leftovers(data_dir);
    remaining.retain(|path| remove_path(path).is_err());
    if remaining.is_empty() {
        remove_record(&record_path(data_dir))
    } else {
        write_leftovers(data_dir, remaining)
    }
}
