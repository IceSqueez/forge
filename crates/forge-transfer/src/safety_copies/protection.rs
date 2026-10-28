use std::path::Path;

use forge_update::update_in_flight;

use crate::restore_journal::RestoreJournal;
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use forge_update::{ApplyState, UpdateJournal};

    use super::protection_for;
    use crate::restore_journal::{RESTORE_JOURNAL_FILE, RestoreJournal};
    use crate::safety_copies::kind::SafetyCopyKind;

    const SNAPSHOT_NAME: &str = "forge-pre-restore-20261022-090507Z-a1b2c3";

    fn journal_referencing(data_dir: &Path, snapshot_dir: PathBuf) {
        RestoreJournal::begin(
            data_dir.join("staging"),
            snapshot_dir,
            data_dir.join("credentials-key"),
        )
        .unwrap()
        .write(data_dir)
        .unwrap();
    }

    #[test]
    fn a_settled_data_dir_protects_nothing() {
        let data = tempfile::tempdir().unwrap();

        let protection = protection_for(data.path());

        assert!(protection.names.is_empty() && protection.kinds.is_empty());
    }

    #[test]
    fn an_unsettled_restore_journal_protects_exactly_the_pre_restore_copy_it_references() {
        let data = tempfile::tempdir().unwrap();
        journal_referencing(data.path(), data.path().join("backups").join(SNAPSHOT_NAME));

        let protection = protection_for(data.path());

        assert_eq!(
            (
                protection.names.into_iter().collect::<Vec<_>>(),
                protection.kinds.is_empty()
            ),
            (vec![OsString::from(SNAPSHOT_NAME)], true)
        );
    }

    #[test]
    fn an_unreadable_restore_journal_protects_every_pre_restore_copy() {
        for garbage in [&b"not json"[..], br#"{"format":0,"phase":"committed"}"#] {
            let data = tempfile::tempdir().unwrap();
            std::fs::write(data.path().join(RESTORE_JOURNAL_FILE), garbage).unwrap();

            let protection = protection_for(data.path());

            assert!(
                protection.kinds.contains(&SafetyCopyKind::PreRestore),
                "{}",
                String::from_utf8_lossy(garbage)
            );
        }
    }

    #[test]
    fn an_update_in_flight_protects_every_pre_update_copy() {
        type PathIn = fn(&Path) -> PathBuf;
        for record in [ApplyState::path_in as PathIn, UpdateJournal::path_in] {
            let data = tempfile::tempdir().unwrap();
            let path = record(data.path());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"{}").unwrap();

            let protection = protection_for(data.path());

            assert_eq!(
                protection.kinds.into_iter().collect::<Vec<_>>(),
                vec![SafetyCopyKind::PreUpdate],
                "{}",
                path.display()
            );
        }
    }
}
