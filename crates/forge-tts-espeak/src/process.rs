use std::process::Stdio;

use crate::error::EspeakError;

pub(crate) fn check_espeak_version() -> Result<(), EspeakError> {
    let status = std::process::Command::new("espeak-ng")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| EspeakError::BinaryNotFound)?;
    if status.success() {
        Ok(())
    } else {
        Err(EspeakError::BinaryNotFound)
    }
}
