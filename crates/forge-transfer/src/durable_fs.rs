use std::io;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::TransferError;
use crate::naming::sibling_with_extension;
use crate::private_file::create_private;

const WRITING_EXTENSION: &str = "writing";

pub(crate) fn exists(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

pub(crate) fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

pub(crate) fn remove_dir_all_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

pub(crate) fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => sync_dir(parent),
        None => Ok(()),
    }
}

#[cfg(unix)]
pub(crate) fn sync_dir(dir: &Path) -> io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
pub(crate) fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

pub(crate) fn copy_atomically(source: &Path, destination: &Path) -> io::Result<()> {
    let mut source = std::fs::File::open(source)?;
    replace_privately(destination, |target| {
        io::copy(&mut source, target).map(drop)
    })
}

fn replace_privately(
    destination: &Path,
    fill: impl FnOnce(&mut std::fs::File) -> io::Result<()>,
) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = sibling_with_extension(destination, WRITING_EXTENSION);
    remove_file_if_present(&temp)?;
    if let Err(e) = fill_then_rename(&temp, destination, fill) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    sync_parent(destination)
}

fn fill_then_rename(
    temp: &Path,
    destination: &Path,
    fill: impl FnOnce(&mut std::fs::File) -> io::Result<()>,
) -> io::Result<()> {
    let mut target = create_private(temp)?;
    fill(&mut target)?;
    target.sync_all()?;
    drop(target);
    std::fs::rename(temp, destination)
}

pub(crate) fn write_record<T: Serialize>(path: &Path, record: &T) -> Result<(), TransferError> {
    let bytes = serde_json::to_vec_pretty(record).map_err(|e| TransferError::MalformedRecord {
        reason: e.to_string(),
    })?;
    replace_privately(path, |file| io::Write::write_all(file, &bytes))?;
    Ok(())
}

pub(crate) fn read_record<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, TransferError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| TransferError::MalformedRecord {
            reason: e.to_string(),
        })
}

pub(crate) fn remove_record(path: &Path) -> Result<(), TransferError> {
    remove_file_if_present(path)?;
    Ok(sync_parent(path)?)
}
