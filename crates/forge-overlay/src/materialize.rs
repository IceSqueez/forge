use std::fs;
use std::path::{Path, PathBuf};

use crate::assets::{CONFIG_FILE, RESERVED_DIRECTORY, RUNTIME_ASSET, RUNTIME_SOURCE};
use crate::document::config_document;
use crate::error::OverlayError;
use crate::instance::OverlayInstance;
use crate::registry::OverlayKindRegistry;

/// Stamped into every config document so a stale record can be spotted and regenerated.
pub const GENERATOR_VERSION: u32 = 1;

const MAX_IDENTITY_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializeReport {
    pub directory: PathBuf,
    pub written: Vec<String>,
    pub preserved: Vec<String>,
    /// A subset of `preserved` whose user copy is gone from disk; nothing was written in its place.
    pub missing_overrides: Vec<String>,
}

/// The runtime is rewritten on every pass; the reserved subtree is generator territory, not the user's.
pub fn ensure_shared_directory(root: &Path) -> Result<PathBuf, OverlayError> {
    let root = ensure_root(root)?;
    let shared = root.join(RESERVED_DIRECTORY);
    reject_symlink(&shared)?;
    create_dir(&shared)?;
    write_atomic(&shared, RUNTIME_ASSET, RUNTIME_SOURCE.as_bytes())?;
    Ok(shared)
}

pub fn materialize_overlay(
    root: &Path,
    instance: &OverlayInstance,
    registry: &OverlayKindRegistry,
) -> Result<MaterializeReport, OverlayError> {
    let descriptor = registry
        .get(&instance.kind_id)
        .ok_or_else(|| OverlayError::UnknownKind(instance.kind_id.clone()))?;

    let directory = overlay_directory(root, &instance.id)?;
    let document = config_document(instance, descriptor)?;

    let mut report = MaterializeReport {
        directory: directory.clone(),
        written: Vec::new(),
        preserved: Vec::new(),
        missing_overrides: Vec::new(),
    };

    for (name, body) in descriptor.page_assets().files() {
        if instance.source_overrides.iter().any(|held| held == name) {
            report.preserved.push(name.to_owned());
            if !directory.join(name).exists() {
                report.missing_overrides.push(name.to_owned());
            }
            continue;
        }
        write_atomic(&directory, name, body.as_bytes())?;
        report.written.push(name.to_owned());
    }

    write_atomic(&directory, CONFIG_FILE, document.as_bytes())?;
    report.written.push(CONFIG_FILE.to_owned());

    Ok(report)
}

/// `Ok(false)` when there was nothing to remove; a symlinked or escaping directory is refused, never followed.
pub fn remove_overlay_directory(root: &Path, id: &str) -> Result<bool, OverlayError> {
    check_identity(id)?;

    let Some(root) = optional_canonicalize(root)? else {
        return Ok(false);
    };
    let directory = root.join(id);
    reject_symlink(&directory)?;

    let Some(resolved) = optional_canonicalize(&directory)? else {
        return Ok(false);
    };
    if !resolved.starts_with(&root) {
        return Err(OverlayError::OutsideRoot {
            path: display(&directory),
        });
    }

    fs::remove_dir_all(&resolved).map_err(|source| OverlayError::Io {
        path: display(&resolved),
        source,
    })?;
    Ok(true)
}

pub(crate) fn overlay_directory(root: &Path, id: &str) -> Result<PathBuf, OverlayError> {
    check_identity(id)?;
    let root = ensure_root(root)?;
    let directory = root.join(id);

    reject_symlink(&directory)?;
    create_dir(&directory)?;

    let resolved = canonicalize(&directory)?;
    if !resolved.starts_with(&root) {
        return Err(OverlayError::OutsideRoot {
            path: display(&directory),
        });
    }
    Ok(resolved)
}

pub(crate) fn check_identity(id: &str) -> Result<(), OverlayError> {
    let safe = !id.is_empty()
        && id.len() <= MAX_IDENTITY_LEN
        && id != RESERVED_DIRECTORY
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');

    if safe {
        Ok(())
    } else {
        Err(OverlayError::UnsafeIdentity(id.to_owned()))
    }
}

fn ensure_root(root: &Path) -> Result<PathBuf, OverlayError> {
    create_dir(root)?;
    canonicalize(root)
}

pub(crate) fn write_atomic(directory: &Path, name: &str, body: &[u8]) -> Result<(), OverlayError> {
    let target = directory.join(name);
    reject_symlink(&target)?;

    let staged = directory.join(format!(".{name}.tmp"));
    reject_symlink(&staged)?;
    fs::write(&staged, body).map_err(|source| OverlayError::Io {
        path: display(&staged),
        source,
    })?;

    fs::rename(&staged, &target).map_err(|source| {
        let _ = fs::remove_file(&staged);
        OverlayError::Io {
            path: display(&target),
            source,
        }
    })
}

pub(crate) fn reject_symlink(path: &Path) -> Result<(), OverlayError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(OverlayError::SymlinkedPath {
            path: display(path),
        }),
        Ok(_) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(OverlayError::Io {
            path: display(path),
            source,
        }),
    }
}

fn create_dir(path: &Path) -> Result<(), OverlayError> {
    fs::create_dir_all(path).map_err(|source| OverlayError::Io {
        path: display(path),
        source,
    })
}

pub(crate) fn optional_canonicalize(path: &Path) -> Result<Option<PathBuf>, OverlayError> {
    match fs::canonicalize(path) {
        Ok(resolved) => Ok(Some(resolved)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(OverlayError::Io {
            path: display(path),
            source,
        }),
    }
}

fn canonicalize(path: &Path) -> Result<PathBuf, OverlayError> {
    fs::canonicalize(path).map_err(|source| OverlayError::Io {
        path: display(path),
        source,
    })
}

pub(crate) fn display(path: &Path) -> String {
    path.display().to_string()
}
