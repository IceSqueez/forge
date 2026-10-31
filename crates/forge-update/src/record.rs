use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::UpdateError;

pub(crate) fn read_record<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, UpdateError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| UpdateError::MalformedRecord(error.to_string()))
}

pub(crate) fn write_record<T: Serialize>(path: &Path, record: &T) -> Result<(), UpdateError> {
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|error| UpdateError::MalformedRecord(error.to_string()))?;
    Ok(forge_durable::write_atomically(path, &bytes)?)
}

pub(crate) fn remove_record(path: &Path) -> Result<(), UpdateError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn require_format(found: u32, supported: u32) -> Result<(), UpdateError> {
    if found == supported {
        Ok(())
    } else {
        Err(UpdateError::MalformedRecord(format!(
            "unsupported record format {found}"
        )))
    }
}
