use std::io;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::TransferError;

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

pub(crate) fn copy_atomically(source: &Path, destination: &Path) -> io::Result<()> {
    forge_durable::discard_leftovers_as_sole_writer(destination)?;
    forge_durable::copy_atomically(source, destination)
}

pub(crate) fn write_record<T: Serialize>(path: &Path, record: &T) -> Result<(), TransferError> {
    let bytes = serde_json::to_vec_pretty(record).map_err(|e| TransferError::MalformedRecord {
        reason: e.to_string(),
    })?;
    forge_durable::discard_leftovers_as_sole_writer(path)?;
    forge_durable::write_atomically(path, &bytes)?;
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
    Ok(forge_durable::sync_parent(path)?)
}
