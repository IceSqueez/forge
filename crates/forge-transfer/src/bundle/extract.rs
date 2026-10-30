use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::bundle::digest::DigestingReader;
use crate::bundle::layout::{ArchiveScope, EntryShape, confined_relative_path, entry_name};
use crate::bundle::manifest::ContentEntry;
use crate::bundle::private_file::create_private;
use crate::error::TransferError;

pub(crate) struct Extractor {
    root: PathBuf,
    scope: ArchiveScope,
    expected: Option<HashMap<String, ContentEntry>>,
    written: HashSet<String>,
    unpacked_bytes: u64,
    entries: usize,
}

impl Extractor {
    pub(crate) fn new(
        root: PathBuf,
        scope: ArchiveScope,
        expected: Option<Vec<ContentEntry>>,
    ) -> Self {
        Self {
            root,
            scope,
            expected: expected.map(|contents| {
                contents
                    .into_iter()
                    .map(|entry| (entry.path.clone(), entry))
                    .collect()
            }),
            written: HashSet::new(),
            unpacked_bytes: 0,
            entries: 0,
        }
    }

    pub(crate) fn extract<R: Read>(
        &mut self,
        entry: &mut tar::Entry<'_, R>,
    ) -> Result<(), TransferError> {
        self.entries += 1;
        let limits = self.scope.limits();
        if self.entries > limits.entries {
            return Err(TransferError::TooManyEntries {
                limit: limits.entries,
            });
        }
        let entry_type = entry.header().entry_type();
        let raw = entry.path().map_err(TransferError::damaged)?.into_owned();
        let shape = if entry_type.is_file() {
            EntryShape::File
        } else if entry_type.is_dir() {
            EntryShape::Dir
        } else {
            return Err(TransferError::UnsafeEntry {
                path: raw.to_string_lossy().into_owned(),
                reason: "only regular files and folders are allowed",
            });
        };
        let relative = confined_relative_path(&raw, shape, self.scope)?;
        match shape {
            EntryShape::Dir => {
                std::fs::create_dir_all(self.root.join(&relative))?;
                Ok(())
            }
            EntryShape::File => {
                let size = entry.size();
                self.extract_file(entry, size, &relative)
            }
        }
    }

    fn extract_file(
        &mut self,
        entry: impl Read,
        size: u64,
        relative: &Path,
    ) -> Result<(), TransferError> {
        let name = entry_name(relative).ok_or_else(|| TransferError::UnsafeEntry {
            path: relative.to_string_lossy().into_owned(),
            reason: "name is not valid UTF-8",
        })?;
        self.admit(&name, size)?;
        let recorded = self.recorded(&name, size)?;

        let destination = self.root.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = create_private(&destination)?;
        let mut reader = DigestingReader::new(entry, size);
        io::copy(&mut reader, &mut file).map_err(TransferError::damaged)?;
        file.flush()?;
        file.sync_all()?;
        if !reader.is_complete() {
            return Err(TransferError::damaged(format!("'{name}' is truncated")));
        }
        let digest = reader.finish_hex();
        if recorded.is_some_and(|recorded| recorded.sha256_hex != digest) {
            return Err(TransferError::DigestMismatch { path: name });
        }
        Ok(())
    }

    fn admit(&mut self, name: &str, size: u64) -> Result<(), TransferError> {
        if !self.written.insert(name.to_owned()) {
            return Err(TransferError::UnsafeEntry {
                path: name.to_owned(),
                reason: "the archive holds this file twice",
            });
        }
        let limits = self.scope.limits();
        if size > limits.entry_bytes {
            return Err(TransferError::EntryTooLarge {
                path: name.to_owned(),
                size,
                limit: limits.entry_bytes,
            });
        }
        self.unpacked_bytes = self.unpacked_bytes.saturating_add(size);
        if self.unpacked_bytes > limits.unpacked_bytes {
            return Err(TransferError::ArchiveTooLarge {
                limit: limits.unpacked_bytes,
            });
        }
        Ok(())
    }

    fn recorded(&self, name: &str, size: u64) -> Result<Option<ContentEntry>, TransferError> {
        let Some(expected) = &self.expected else {
            return Ok(None);
        };
        let recorded =
            expected
                .get(name)
                .cloned()
                .ok_or_else(|| TransferError::ContentsMismatch {
                    path: name.to_owned(),
                })?;
        if recorded.size != size {
            return Err(TransferError::DigestMismatch {
                path: name.to_owned(),
            });
        }
        Ok(Some(recorded))
    }

    pub(crate) fn finish(self) -> Result<(), TransferError> {
        let Some(expected) = self.expected else {
            return Ok(());
        };
        match expected.keys().find(|name| !self.written.contains(*name)) {
            Some(missing) => Err(TransferError::ContentsMismatch {
                path: missing.clone(),
            }),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::limits::{
        MAX_SHARED_ARCHIVE_ENTRIES, MAX_SHARED_ENTRY_BYTES, MAX_SHARED_UNPACKED_BYTES,
    };

    const PACK_FILE: &str = "pack/documents.json";
    const SETUP_FILE: &str = "setup/documents.json";
    const BACKUP_FILE: &str = "forge/forge.db";

    fn header_only(name: &str, declared_size: u64) -> Vec<u8> {
        let mut header = tar::Header::new_gnu();
        header.set_path(name).unwrap();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(declared_size);
        header.set_mode(0o600);
        header.set_cksum();
        header.as_bytes().to_vec()
    }

    fn extract_declared(
        scope: ArchiveScope,
        name: &str,
        declared_size: u64,
        prepare: impl FnOnce(&mut Extractor),
    ) -> Result<(), TransferError> {
        let root = tempfile::tempdir().unwrap();
        let mut extractor = Extractor::new(root.path().to_path_buf(), scope, None);
        prepare(&mut extractor);
        let bytes = header_only(name, declared_size);
        let mut archive = tar::Archive::new(std::io::Cursor::new(bytes));
        let mut entries = archive.entries().unwrap();
        let mut entry = entries.next().unwrap().unwrap();
        extractor.extract(&mut entry)
    }

    #[test]
    fn a_shared_scope_entry_over_the_shared_cap_is_refused_where_a_backup_entry_of_that_size_is_not()
     {
        let size = MAX_SHARED_ENTRY_BYTES + 1;

        let pack = extract_declared(ArchiveScope::Pack, PACK_FILE, size, |_| {});
        let setup = extract_declared(ArchiveScope::Setup, SETUP_FILE, size, |_| {});
        let backup = extract_declared(ArchiveScope::Backup, BACKUP_FILE, size, |_| {});

        for refused in [pack, setup] {
            assert!(
                matches!(refused, Err(TransferError::EntryTooLarge { limit, .. }) if limit == MAX_SHARED_ENTRY_BYTES),
                "{refused:?}"
            );
        }
        assert!(
            !matches!(backup, Err(TransferError::EntryTooLarge { .. })),
            "{backup:?}"
        );
    }

    #[test]
    fn a_shared_scope_entry_at_the_shared_cap_passes_the_size_check() {
        let at_cap = extract_declared(
            ArchiveScope::Pack,
            PACK_FILE,
            MAX_SHARED_ENTRY_BYTES,
            |_| {},
        );

        assert!(
            !matches!(at_cap, Err(TransferError::EntryTooLarge { .. })),
            "{at_cap:?}"
        );
    }

    #[test]
    fn the_entry_past_the_shared_entry_count_cap_is_refused_for_pack_and_setup_but_not_backup() {
        let past_cap = |scope, name| {
            extract_declared(scope, name, 0, |extractor| {
                extractor.entries = MAX_SHARED_ARCHIVE_ENTRIES;
            })
        };

        let pack = past_cap(ArchiveScope::Pack, PACK_FILE);
        let setup = past_cap(ArchiveScope::Setup, SETUP_FILE);
        let backup = past_cap(ArchiveScope::Backup, BACKUP_FILE);

        for refused in [pack, setup] {
            assert!(
                matches!(refused, Err(TransferError::TooManyEntries { limit }) if limit == MAX_SHARED_ARCHIVE_ENTRIES),
                "{refused:?}"
            );
        }
        assert!(backup.is_ok(), "{backup:?}");
    }

    #[test]
    fn the_last_entry_inside_the_shared_entry_count_cap_is_not_refused() {
        let inside = extract_declared(ArchiveScope::Pack, PACK_FILE, 0, |extractor| {
            extractor.entries = MAX_SHARED_ARCHIVE_ENTRIES - 1;
        });

        assert!(inside.is_ok(), "{inside:?}");
    }

    #[test]
    fn a_pack_whose_total_unpacked_size_passes_the_shared_cap_is_refused() {
        let over = extract_declared(ArchiveScope::Pack, PACK_FILE, 1, |extractor| {
            extractor.unpacked_bytes = MAX_SHARED_UNPACKED_BYTES;
        });

        assert!(
            matches!(over, Err(TransferError::ArchiveTooLarge { limit }) if limit == MAX_SHARED_UNPACKED_BYTES),
            "{over:?}"
        );
    }
}
