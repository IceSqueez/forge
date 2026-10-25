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

pub(crate) fn file_bytes(path: &Path) -> u64 {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.file_type().is_file())
        .map_or(0, |metadata| metadata.len())
}

pub(crate) fn directory_bytes(path: &Path) -> u64 {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use forge_storage::DATABASE_FILE_NAME;
    use forge_storage::paths::{
        ASSETS_DIR_NAME, BACKUPS_DIR_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME, TTS_DIR_NAME,
    };

    use super::{LOGS_DIR_NAME, StorageUsed, measure};

    fn sized(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0_u8; bytes]).unwrap();
    }

    fn populated() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let data = root.path();
        sized(&data.join(DATABASE_FILE_NAME), 100);
        sized(&data.join(format!("{DATABASE_FILE_NAME}-wal")), 20);
        sized(&data.join(format!("{DATABASE_FILE_NAME}-shm")), 3);
        sized(
            &data.join(MEDIA_DIR_NAME).join("nested").join("clip.ogg"),
            40,
        );
        sized(&data.join(MEDIA_DIR_NAME).join("intro.ogg"), 2);
        sized(&data.join(TTS_DIR_NAME).join("voice.onnx"), 50);
        sized(&data.join(OVERLAYS_DIR_NAME).join("o.html"), 6);
        sized(&data.join(ASSETS_DIR_NAME).join("a.png"), 7);
        sized(&data.join(LOGS_DIR_NAME).join("forge.log"), 8);
        sized(&data.join(BACKUPS_DIR_NAME).join("old.forgebundle"), 9);
        root
    }

    #[test]
    fn storage_used_sums_each_category_including_database_sidecars() {
        let data = populated();

        assert_eq!(
            measure(data.path(), None),
            StorageUsed {
                database: 123,
                media: 42,
                voices: 50,
                overlays: 6,
                assets: 7,
                logs: 8,
                backups: 9,
                destination_outside_data_dir: 0,
            }
        );
    }

    #[test]
    fn an_outside_destination_counts_only_its_bundles() {
        let data = populated();
        let usb = tempfile::tempdir().unwrap();
        sized(&usb.path().join("forge-scheduled-a.forgebundle"), 30);
        sized(&usb.path().join("holiday.mp4"), 1000);

        let used = measure(data.path(), Some(usb.path()));

        assert_eq!(used.destination_outside_data_dir, 30);
    }

    #[test]
    fn a_destination_inside_the_data_dir_is_not_counted_twice() {
        let data = populated();

        let used = measure(data.path(), Some(&data.path().join(BACKUPS_DIR_NAME)));

        assert_eq!(used.destination_outside_data_dir, 0);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_content_is_not_followed() {
        let data = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        sized(&elsewhere.path().join("huge.bin"), 5000);
        std::fs::create_dir_all(data.path().join(MEDIA_DIR_NAME)).unwrap();
        std::os::unix::fs::symlink(
            elsewhere.path(),
            data.path().join(MEDIA_DIR_NAME).join("linked-dir"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            elsewhere.path().join("huge.bin"),
            data.path().join(MEDIA_DIR_NAME).join("linked-file"),
        )
        .unwrap();

        assert_eq!(measure(data.path(), None).media, 0);
    }
}
