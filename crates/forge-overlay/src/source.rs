use std::fs;
use std::path::Path;

use crate::assets::OVERRIDABLE_FILES;
use crate::error::OverlayError;
use crate::materialize::{
    check_identity, display, existing_overlay_directory, overlay_directory, reject_symlink,
    write_atomic,
};

pub fn read_overlay_source(
    root: &Path,
    id: &str,
    name: &str,
) -> Result<Option<String>, OverlayError> {
    check_identity(id)?;
    check_overridable(name)?;

    let Some(resolved) = existing_overlay_directory(root, id)? else {
        return Ok(None);
    };

    let target = resolved.join(name);
    reject_symlink(&target)?;
    match fs::read_to_string(&target) {
        Ok(body) => Ok(Some(body)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(OverlayError::Io {
            path: display(&target),
            source,
        }),
    }
}

pub fn write_overlay_source(
    root: &Path,
    id: &str,
    name: &str,
    body: &str,
) -> Result<(), OverlayError> {
    check_overridable(name)?;
    let directory = overlay_directory(root, id)?;
    write_atomic(&directory, name, body.as_bytes())
}

fn check_overridable(name: &str) -> Result<(), OverlayError> {
    if OVERRIDABLE_FILES.contains(&name) {
        Ok(())
    } else {
        Err(OverlayError::NotOverridable(name.to_owned()))
    }
}
