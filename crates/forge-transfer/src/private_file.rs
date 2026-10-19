use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(unix)]
const OWNER_ONLY_MODE: u32 = 0o600;

#[cfg(unix)]
pub(crate) fn create_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY_MODE)
        .open(path)
}

#[cfg(not(unix))]
pub(crate) fn create_private(path: &Path) -> io::Result<File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}
