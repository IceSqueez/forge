use std::fs::OpenOptions;
use std::path::Path;

const PROBE_PREFIX: &str = ".forge-write-probe-";

pub(super) fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!("{PROBE_PREFIX}{}", std::process::id()));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(file) => {
            drop(file);
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}
