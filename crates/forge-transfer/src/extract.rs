use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::digest::DigestingReader;
use crate::error::TransferError;
use crate::layout::{EntryShape, confined_relative_path, entry_name};
use crate::limits::{MAX_ARCHIVE_ENTRIES, MAX_ENTRY_BYTES, MAX_UNPACKED_BYTES};
use crate::manifest::ContentEntry;
use crate::private_file::create_private;

pub(crate) struct Extractor {
    root: PathBuf,
    expected: Option<HashMap<String, ContentEntry>>,
    written: HashSet<String>,
    unpacked_bytes: u64,
    entries: usize,
}

impl Extractor {
    pub(crate) fn new(root: PathBuf, expected: Option<Vec<ContentEntry>>) -> Self {
        Self {
            root,
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
        if self.entries > MAX_ARCHIVE_ENTRIES {
            return Err(TransferError::TooManyEntries {
                limit: MAX_ARCHIVE_ENTRIES,
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
        let relative = confined_relative_path(&raw, shape)?;
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
        if size > MAX_ENTRY_BYTES {
            return Err(TransferError::EntryTooLarge {
                path: name.to_owned(),
                size,
                limit: MAX_ENTRY_BYTES,
            });
        }
        self.unpacked_bytes = self.unpacked_bytes.saturating_add(size);
        if self.unpacked_bytes > MAX_UNPACKED_BYTES {
            return Err(TransferError::ArchiveTooLarge {
                limit: MAX_UNPACKED_BYTES,
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
