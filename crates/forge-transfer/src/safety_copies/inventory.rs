use std::path::{Path, PathBuf};

use time::OffsetDateTime;

use crate::bundle::fs_usage::{directory_bytes, file_bytes};
use crate::bundle::naming::backups_dir;
use crate::safety_copies::kind::{SafetyCopyKind, classify};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SafetyCopy {
    pub path: PathBuf,
    pub kind: SafetyCopyKind,
    pub created_at: OffsetDateTime,
    pub bytes: u64,
}

pub(crate) fn inventory(data_dir: &Path) -> std::io::Result<Vec<SafetyCopy>> {
    let entries = match std::fs::read_dir(backups_dir(data_dir)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut copies = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !(file_type.is_dir() || file_type.is_file()) {
            continue;
        }
        let Some(classified) = classify(&entry.file_name(), file_type.is_dir()) else {
            continue;
        };
        let path = entry.path();
        let bytes = if file_type.is_dir() {
            directory_bytes(&path)
        } else {
            file_bytes(&path)
        };
        copies.push(SafetyCopy {
            path,
            kind: classified.kind,
            created_at: classified.created_at,
            bytes,
        });
    }
    Ok(copies)
}
