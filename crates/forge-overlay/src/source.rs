use std::fs;
use std::path::Path;

use crate::assets::OVERRIDABLE_FILES;
use crate::error::OverlayError;
use crate::materialize::{
    check_identity, display, optional_canonicalize, overlay_directory, reject_symlink, write_atomic,
};

/// `Ok(None)` while the root, the overlay directory or the file is absent, so a page that was
/// never written is not reported as a read failure.
pub fn read_overlay_source(
    root: &Path,
    id: &str,
    name: &str,
) -> Result<Option<String>, OverlayError> {
    check_identity(id)?;
    check_overridable(name)?;

    let Some(root) = optional_canonicalize(root)? else {
        return Ok(None);
    };
    let directory = root.join(id);
    reject_symlink(&directory)?;

    let Some(resolved) = optional_canonicalize(&directory)? else {
        return Ok(None);
    };
    if !resolved.starts_with(&root) {
        return Err(OverlayError::OutsideRoot {
            path: display(&directory),
        });
    }

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

/// Writes one user-owned page file; keeping the record's override list in step is the caller's job.
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
