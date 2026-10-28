use std::io;
use std::path::Path;

use crate::apply_state::ApplyState;
use crate::journal::UpdateJournal;

pub fn update_in_flight(data_dir: &Path) -> io::Result<bool> {
    Ok(record_present(&ApplyState::path_in(data_dir))?
        || record_present(&UpdateJournal::path_in(data_dir))?)
}

fn record_present(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
