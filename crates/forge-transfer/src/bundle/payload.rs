use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::bundle::category::{BackupCategory, directories_of};
use crate::bundle::digest::{DigestingReader, digest_reader};
use crate::bundle::layout::{archive_name_of, credentials_key_entry, database_entry};
use crate::bundle::manifest::ContentEntry;
use crate::error::TransferError;

const ARCHIVED_FILE_MODE: u32 = 0o600;

pub(crate) struct PayloadFile {
    source: PathBuf,
    pub(crate) entry: ContentEntry,
}

pub(crate) fn collect_payload(
    data_dir: &Path,
    database_snapshot: &Path,
    credentials_key: &Path,
    categories: &[BackupCategory],
) -> Result<Vec<PayloadFile>, TransferError> {
    let mut files = vec![digested(database_snapshot, database_entry())?];
    if credentials_key.is_file() {
        files.push(digested(credentials_key, credentials_key_entry())?);
    }
    for dir in directories_of(categories) {
        walk(data_dir, Path::new(dir), &mut files)?;
    }
    Ok(files)
}

fn walk(data_dir: &Path, relative: &Path, out: &mut Vec<PayloadFile>) -> Result<(), TransferError> {
    let absolute = data_dir.join(relative);
    if !absolute.is_dir() {
        return Ok(());
    }
    let mut children = std::fs::read_dir(&absolute)?
        .map(|entry| entry.map(|e| e.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort();
    for name in children {
        let child = relative.join(&name);
        let kind = std::fs::symlink_metadata(data_dir.join(&child))?.file_type();
        if kind.is_dir() {
            walk(data_dir, &child, out)?;
        } else if kind.is_file() {
            let archive_name =
                archive_name_of(&child).ok_or_else(|| TransferError::UnsupportedFileName {
                    path: child.to_string_lossy().into_owned(),
                })?;
            out.push(digested(&data_dir.join(&child), archive_name)?);
        }
    }
    Ok(())
}

pub(crate) fn digested(source: &Path, path: String) -> Result<PayloadFile, TransferError> {
    let (size, sha256_hex) = digest_reader(File::open(source)?)?;
    Ok(PayloadFile {
        source: source.to_path_buf(),
        entry: ContentEntry {
            path,
            size,
            sha256_hex,
        },
    })
}

pub(crate) fn append_bytes<W: Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    bytes: &[u8],
) -> Result<(), TransferError> {
    let mut header = file_header(bytes.len() as u64);
    builder.append_data(&mut header, name, bytes)?;
    Ok(())
}

pub(crate) fn append_file<W: Write>(
    builder: &mut tar::Builder<W>,
    name: &str,
    source: &Path,
) -> Result<(), TransferError> {
    let file = File::open(source)?;
    let mut header = file_header(file.metadata()?.len());
    builder.append_data(&mut header, name, file)?;
    Ok(())
}

pub(crate) fn append_verified<W: Write>(
    builder: &mut tar::Builder<W>,
    file: &PayloadFile,
) -> Result<(), TransferError> {
    let changed = || TransferError::DataChangedDuringBackup {
        path: file.entry.path.clone(),
    };
    let mut reader = DigestingReader::new(File::open(&file.source)?, file.entry.size);
    let mut header = file_header(file.entry.size);
    let appended = builder.append_data(&mut header, &file.entry.path, &mut reader);
    if !reader.is_complete() {
        return Err(changed());
    }
    appended?;
    if reader.finish_hex() != file.entry.sha256_hex {
        return Err(changed());
    }
    Ok(())
}

fn file_header(size: u64) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(size);
    header.set_mode(ARCHIVED_FILE_MODE);
    header
}
