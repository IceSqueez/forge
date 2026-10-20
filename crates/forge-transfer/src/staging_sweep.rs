use std::path::Path;

use crate::durable_fs::remove_dir_all_if_present;
use crate::error::TransferError;
use crate::pack::pack_staging_root;
use crate::setup::setup_staging_root;

pub fn sweep_import_staging(data_dir: &Path) -> Result<(), TransferError> {
    let packs = remove_dir_all_if_present(&pack_staging_root(data_dir));
    let setups = remove_dir_all_if_present(&setup_staging_root(data_dir));
    packs.and(setups).map_err(TransferError::from)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::sweep_import_staging;
    use crate::{RESTORE_MARKER_FILE, pack_staging_root, restore_staging_root, setup_staging_root};

    fn leftover_in(root: &Path) -> PathBuf {
        let staged = root.join("01JLEFTOVER");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("manifest.json"), b"{}").unwrap();
        staged
    }

    #[test]
    fn sweeping_removes_pack_and_setup_staging_but_keeps_a_pending_restore() {
        let data = tempfile::tempdir().unwrap();
        let pack = pack_staging_root(data.path());
        let setup = setup_staging_root(data.path());
        leftover_in(&pack);
        leftover_in(&setup);
        let restore = leftover_in(&restore_staging_root(data.path()));
        let marker = data.path().join(RESTORE_MARKER_FILE);
        std::fs::write(&marker, b"{}").unwrap();

        sweep_import_staging(data.path()).unwrap();

        assert_eq!(
            [
                pack.exists(),
                setup.exists(),
                restore.exists(),
                marker.exists()
            ],
            [false, false, true, true]
        );
    }

    #[test]
    fn sweeping_a_data_dir_without_staging_succeeds() {
        let data = tempfile::tempdir().unwrap();

        assert!(sweep_import_staging(data.path()).is_ok());
    }

    #[test]
    fn a_pack_staging_that_cannot_be_removed_still_sweeps_setups_and_reports_the_failure() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(pack_staging_root(data.path()), b"not a directory").unwrap();
        let setup = setup_staging_root(data.path());
        leftover_in(&setup);

        let swept = sweep_import_staging(data.path());

        assert_eq!((swept.is_err(), setup.exists()), (true, false));
    }
}
