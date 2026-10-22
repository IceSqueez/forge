use std::path::Path;

use crate::restore_journal::RestoreJournal;
use crate::restore_marker::update_in_flight;
use crate::safety_copies::kind::SafetyCopyKind;
use crate::safety_copies::selection::Protection;

pub(crate) fn protection_for(data_dir: &Path) -> Protection {
    let mut protection = Protection::default();
    match RestoreJournal::read(data_dir) {
        Ok(None) => {}
        Ok(Some(journal)) => {
            if let Some(name) = journal.snapshot_dir.file_name() {
                protection.names.insert(name.to_owned());
            }
        }
        Err(_) => {
            protection.kinds.insert(SafetyCopyKind::PreRestore);
        }
    }
    if update_in_flight(data_dir).unwrap_or(true) {
        protection.kinds.insert(SafetyCopyKind::PreUpdate);
    }
    protection
}
