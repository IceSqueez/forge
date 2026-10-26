use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::UpdateError;

const TEMP_SUFFIX: &str = ".tmp-";

pub(crate) async fn write_atomically(path: &Path, bytes: Vec<u8>) -> Result<(), UpdateError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || write_atomically_blocking(&path, &bytes))
        .await
        .map_err(|error| UpdateError::Io(std::io::Error::other(error.to_string())))?
}

pub(crate) fn write_atomically_blocking(path: &Path, bytes: &[u8]) -> Result<(), UpdateError> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    std::fs::create_dir_all(dir)?;
    let temp = temp_path(path);
    let written = write_and_sync(&temp, bytes).and_then(|()| std::fs::rename(&temp, path));
    if let Err(error) = written {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "best-effort cleanup; the write error is returned"
        )]
        let _ = std::fs::remove_file(&temp);
        return Err(error.into());
    }
    sync_dir(dir)?;
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!("{TEMP_SUFFIX}{}", std::process::id()));
    path.with_file_name(name)
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(unix)]
pub(crate) fn sync_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
pub(crate) fn sync_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}
