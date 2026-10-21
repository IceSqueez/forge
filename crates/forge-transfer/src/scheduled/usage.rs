use std::path::Path;

use forge_storage::DATABASE_FILE_NAME;
use forge_storage::paths::BACKUPS_DIR_NAME;

use crate::category::BackupCategory;
use crate::naming::BUNDLE_EXTENSION;
use crate::scheduled::destination::is_within;

const LOGS_DIR_NAME: &str = "logs";
const DATABASE_SIDECAR_SUFFIXES: [&str; 2] = ["-wal", "-shm"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StorageUsed {
    pub database: u64,
    pub media: u64,
    pub voices: u64,
    pub overlays: u64,
    pub assets: u64,
    pub logs: u64,
    pub backups: u64,
    pub destination_outside_data_dir: u64,
}

pub(crate) fn measure(data_dir: &Path, destination: Option<&Path>) -> StorageUsed {
    let category_bytes = |category: BackupCategory| {
        category
            .directory()
            .map_or(0, |dir| directory_bytes(&data_dir.join(dir)))
    };
    StorageUsed {
        database: database_bytes(data_dir),
        media: category_bytes(BackupCategory::Media),
        voices: category_bytes(BackupCategory::Voices),
        overlays: category_bytes(BackupCategory::Overlays),
        assets: category_bytes(BackupCategory::Assets),
        logs: directory_bytes(&data_dir.join(LOGS_DIR_NAME)),
        backups: directory_bytes(&data_dir.join(BACKUPS_DIR_NAME)),
        destination_outside_data_dir: destination
            .filter(|folder| !is_within(folder, data_dir))
            .map_or(0, bundle_bytes),
    }
}

fn database_bytes(data_dir: &Path) -> u64 {
    std::iter::once(String::new())
        .chain(DATABASE_SIDECAR_SUFFIXES.iter().map(|s| (*s).to_owned()))
        .map(|suffix| file_bytes(&data_dir.join(format!("{DATABASE_FILE_NAME}{suffix}"))))
        .sum()
}

fn file_bytes(path: &Path) -> u64 {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.file_type().is_file())
        .map_or(0, |metadata| metadata.len())
}

fn directory_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let child = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => directory_bytes(&child),
                Ok(kind) if kind.is_file() => file_bytes(&child),
                _ => 0,
            }
        })
        .sum()
}

fn bundle_bytes(folder: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == BUNDLE_EXTENSION))
        .map(|path| file_bytes(&path))
        .sum()
}
