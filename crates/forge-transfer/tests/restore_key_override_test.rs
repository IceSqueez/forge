#![allow(clippy::unwrap_used)]

use forge_paths::CREDENTIALS_KEY_FILE_NAME;
use forge_transfer::{
    ARCHIVE_ROOT, RESTORE_MARKER_FILE, RestoreOutcome, apply_pending_restore, restore_staging_root,
};
use tempfile::TempDir;

mod common;
use common::write_file;

const STAGE_NAME: &str = "forge-backup-20261019-120000Z-abcdef";
const KEY_OVERRIDE_TEST: &str =
    "restore_writes_the_key_to_the_overridden_key_file_and_leaves_the_data_dir_one_alone";

#[test]
fn restore_writes_the_key_to_the_overridden_key_file_and_leaves_the_data_dir_one_alone() {
    let Some(overridden) = common::key_file_of_this_child_run() else {
        let keys = TempDir::new().unwrap();
        common::rerun_in_child_with_key_file(KEY_OVERRIDE_TEST, &keys.path().join("forge.key"));
        return;
    };
    let data = TempDir::new().unwrap();
    write_file(&overridden, b"original overridden key");
    let decoy = data.path().join(CREDENTIALS_KEY_FILE_NAME);
    write_file(&decoy, b"decoy key in the data dir");
    write_file(&data.path().join("forge.db"), b"original database");
    let payload = restore_staging_root(data.path())
        .join(STAGE_NAME)
        .join(ARCHIVE_ROOT);
    write_file(&payload.join("forge.db"), b"restored database");
    write_file(
        &payload.join(CREDENTIALS_KEY_FILE_NAME),
        b"restored credentials key",
    );
    write_file(
        &data.path().join(RESTORE_MARKER_FILE),
        format!(r#"{{"format":1,"staging":"{STAGE_NAME}"}}"#).as_bytes(),
    );

    let outcome = apply_pending_restore(data.path()).unwrap();

    let RestoreOutcome::Applied {
        pre_restore_snapshot,
    } = outcome
    else {
        panic!("expected Applied, got {outcome:?}");
    };
    assert_eq!(
        (
            std::fs::read(&overridden).unwrap(),
            std::fs::read(&decoy).unwrap(),
            std::fs::read(pre_restore_snapshot.join(CREDENTIALS_KEY_FILE_NAME)).unwrap(),
        ),
        (
            b"restored credentials key".to_vec(),
            b"decoy key in the data dir".to_vec(),
            b"original overridden key".to_vec(),
        )
    );
}
