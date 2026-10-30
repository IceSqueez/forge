use std::path::{Component, Path, PathBuf};

use forge_paths::{
    ASSETS_DIR_NAME, CREDENTIALS_KEY_FILE_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME, TTS_DIR_NAME,
};
use forge_storage::DATABASE_FILE_NAME;

use crate::bundle::document_layout::{PACK_LAYOUT, SETUP_LAYOUT, is_documents_scope};
use crate::bundle::limits::{
    MAX_ARCHIVE_ENTRIES, MAX_ENTRY_BYTES, MAX_SHARED_ARCHIVE_ENTRIES, MAX_SHARED_ENTRY_BYTES,
    MAX_SHARED_UNPACKED_BYTES, MAX_UNPACKED_BYTES,
};
use crate::error::TransferError;

pub const ARCHIVE_ROOT: &str = "forge";

pub const INCLUDED_DIRS: [&str; 4] = [
    MEDIA_DIR_NAME,
    OVERLAYS_DIR_NAME,
    TTS_DIR_NAME,
    ASSETS_DIR_NAME,
];

pub(crate) const ARCHIVE_SEPARATOR: &str = "/";
const FORBIDDEN_NAME_CHARS: [char; 9] = [':', '\\', '<', '>', '"', '|', '?', '*', '/'];
const RESERVED_DEVICE_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub(crate) fn is_portable_segment(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment).trim_end();
    !segment.contains(FORBIDDEN_NAME_CHARS)
        && !segment.chars().any(char::is_control)
        && !segment.ends_with(['.', ' '])
        && !RESERVED_DEVICE_STEMS
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(stem))
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveScope {
    Backup,
    Pack,
    Setup,
}

pub(crate) struct ScopeLimits {
    pub(crate) entry_bytes: u64,
    pub(crate) unpacked_bytes: u64,
    pub(crate) entries: usize,
}

impl ArchiveScope {
    pub(crate) fn limits(self) -> ScopeLimits {
        match self {
            Self::Backup => ScopeLimits {
                entry_bytes: MAX_ENTRY_BYTES,
                unpacked_bytes: MAX_UNPACKED_BYTES,
                entries: MAX_ARCHIVE_ENTRIES,
            },
            Self::Pack | Self::Setup => ScopeLimits {
                entry_bytes: MAX_SHARED_ENTRY_BYTES,
                unpacked_bytes: MAX_SHARED_UNPACKED_BYTES,
                entries: MAX_SHARED_ARCHIVE_ENTRIES,
            },
        }
    }

    fn root(self) -> &'static str {
        match self {
            Self::Backup => ARCHIVE_ROOT,
            Self::Pack => PACK_LAYOUT.root(),
            Self::Setup => SETUP_LAYOUT.root(),
        }
    }

    fn admits(self, rest: &[&str], shape: EntryShape) -> bool {
        match self {
            Self::Backup => is_backup_scope(rest, shape),
            Self::Pack | Self::Setup => is_documents_scope(rest, shape),
        }
    }
}

pub(crate) fn confined_relative_path(
    raw: &Path,
    shape: EntryShape,
    scope: ArchiveScope,
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
                Some(text) if !is_portable_segment(text) => {
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
    if root != scope.root() {
        return Err(unsafe_entry("path is outside the forge folder"));
    }
    if !scope.admits(rest, shape) {
        return Err(unsafe_entry("path is not part of this kind of forge file"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_segments_accept_ordinary_and_non_ascii_names() {
        for good in [
            "readme.txt",
            "payload.css",
            "COM10",
            "COM1x.png",
            "console.log",
            "auxiliary",
            "a.b.c",
            "файл.png",
            ".hidden",
        ] {
            assert!(is_portable_segment(good), "expected portable: {good:?}");
        }
    }

    #[test]
    fn portable_segments_reject_windows_reserved_device_names_with_any_extension_and_case() {
        for bad in [
            "CON",
            "con",
            "aux.png",
            "AUX.tar.gz",
            "COM1",
            "com9.txt",
            "LPT1",
            "lpt9",
            "NUL.x",
            "PRN",
            "CON .txt",
        ] {
            assert!(!is_portable_segment(bad), "expected rejected: {bad:?}");
        }
    }

    #[test]
    fn portable_segments_reject_trailing_dot_and_space() {
        for bad in ["name.", "name ", "...", "name. "] {
            assert!(!is_portable_segment(bad), "expected rejected: {bad:?}");
        }
    }

    #[test]
    fn portable_segments_reject_every_windows_forbidden_character_and_controls() {
        for forbidden in [
            '<', '>', ':', '"', '|', '?', '*', '\\', '/', '\u{0}', '\n', '\u{7f}',
        ] {
            let name = format!("a{forbidden}b");
            assert!(!is_portable_segment(&name), "expected rejected: {name:?}");
        }
    }

    #[test]
    fn confined_relative_path_refuses_a_reserved_device_name_segment() {
        let refused = confined_relative_path(
            Path::new("pack/overlays/0/aux.png"),
            EntryShape::File,
            ArchiveScope::Pack,
        );

        assert!(
            matches!(refused, Err(TransferError::UnsafeEntry { .. })),
            "{refused:?}"
        );
    }
}
