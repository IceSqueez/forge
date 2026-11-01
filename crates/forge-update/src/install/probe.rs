use std::fs::OpenOptions;
use std::path::Path;

use crate::cleanup::log_failed_cleanup;

const PROBE_PREFIX: &str = ".forge-write-probe-";

pub(super) fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!("{PROBE_PREFIX}{}", std::process::id()));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(file) => {
            drop(file);
            log_failed_cleanup(
                "the probe only tests whether the directory is writable",
                std::fs::remove_file(&probe),
            );
            true
        }
        Err(_) => false,
    }
}
