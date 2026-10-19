#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use forge_storage::paths::CREDENTIALS_KEY_FILE_NAME;
use forge_transfer::{
    ARCHIVE_ROOT, RESTORE_MARKER_FILE, RestoreOutcome, apply_pending_restore, restore_staging_root,
};
use tempfile::TempDir;

const KEY_FILE_ENV: &str = "FORGE_CREDENTIAL_KEY_FILE";
const STAGE_NAME: &str = "forge-backup-20261019-120000Z-abcdef";

fn write_file(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[allow(unsafe_code)]
#[test]
fn restore_writes_the_key_to_the_overridden_key_file_and_leaves_the_data_dir_one_alone() {
    let data = TempDir::new().unwrap();
    let keys = TempDir::new().unwrap();
    let overridden = keys.path().join("forge.key");
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
    // SAFETY: this is the only test in this binary, so no other thread reads or writes the environment while it runs.
    unsafe { std::env::set_var(KEY_FILE_ENV, &overridden) };

    let outcome = apply_pending_restore(data.path()).unwrap();

    // SAFETY: still the only test in this binary; the apply above has returned.
    unsafe { std::env::remove_var(KEY_FILE_ENV) };
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
