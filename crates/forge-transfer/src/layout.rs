use std::path::{Component, Path, PathBuf};

use forge_storage::DATABASE_FILE_NAME;
use forge_storage::paths::{
    ASSETS_DIR_NAME, CREDENTIALS_KEY_FILE_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME, TTS_DIR_NAME,
};

use crate::error::TransferError;

pub const ARCHIVE_ROOT: &str = "forge";

pub const INCLUDED_DIRS: [&str; 4] = [
    MEDIA_DIR_NAME,
    OVERLAYS_DIR_NAME,
    TTS_DIR_NAME,
    ASSETS_DIR_NAME,
];

const ARCHIVE_SEPARATOR: &str = "/";
const FORBIDDEN_NAME_CHARS: [char; 2] = [':', '\\'];

pub(crate) fn database_entry() -> String {
    archive_path([DATABASE_FILE_NAME])
}

pub(crate) fn credentials_key_entry() -> String {
    archive_path([CREDENTIALS_KEY_FILE_NAME])
}

pub(crate) fn archive_path<'a>(segments: impl IntoIterator<Item = &'a str>) -> String {
    std::iter::once(ARCHIVE_ROOT)
        .chain(segments)
        .collect::<Vec<_>>()
        .join(ARCHIVE_SEPARATOR)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryShape {
    File,
    Dir,
}

pub(crate) fn confined_relative_path(
    raw: &Path,
    shape: EntryShape,
) -> Result<PathBuf, TransferError> {
    let shown = raw.to_string_lossy().into_owned();
    let unsafe_entry = |reason| TransferError::UnsafeEntry {
        path: shown.clone(),
        reason,
    };
    let mut segments = Vec::new();
    for component in raw.components() {
        match component {
            Component::Normal(segment) => match segment.to_str() {
                Some(text) if text.contains(FORBIDDEN_NAME_CHARS) => {
                    return Err(unsafe_entry("name holds a reserved character"));
                }
                Some(text) => segments.push(text),
                None => return Err(unsafe_entry("name is not valid UTF-8")),
            },
            Component::CurDir => {}
            Component::ParentDir => return Err(unsafe_entry("path climbs out of the archive")),
            Component::RootDir | Component::Prefix(_) => {
                return Err(unsafe_entry("path is absolute"));
            }
        }
    }
    let Some((&root, rest)) = segments.split_first() else {
        return Err(unsafe_entry("path is empty"));
    };
    if root != ARCHIVE_ROOT {
        return Err(unsafe_entry("path is outside the forge folder"));
    }
    if !is_backup_scope(rest, shape) {
        return Err(unsafe_entry("path is not part of a forge backup"));
    }
    Ok(segments.iter().collect())
}

fn is_backup_scope(rest: &[&str], shape: EntryShape) -> bool {
    match (rest, shape) {
        ([], EntryShape::Dir) => true,
        ([name], EntryShape::File) => {
            *name == DATABASE_FILE_NAME || *name == CREDENTIALS_KEY_FILE_NAME
        }
        ([dir, ..], _) => INCLUDED_DIRS.contains(dir),
        _ => false,
    }
}

pub(crate) fn archive_name_of(relative_to_data_dir: &Path) -> Option<String> {
    segments_of(relative_to_data_dir).map(archive_path)
}

pub(crate) fn entry_name(confined: &Path) -> Option<String> {
    segments_of(confined).map(|segments| segments.join(ARCHIVE_SEPARATOR))
}

fn segments_of(path: &Path) -> Option<Vec<&str>> {
    path.components()
        .map(|component| match component {
            Component::Normal(segment) => segment.to_str(),
            _ => None,
        })
        .collect()
}
