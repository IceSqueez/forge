use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use forge_storage::DataProvider;
use time::OffsetDateTime;

use forge_storage::DATABASE_FILE_NAME;
pub use forge_storage::paths::BACKUPS_DIR_NAME;
use forge_storage::paths::{
    ASSETS_DIR_NAME, CREDENTIALS_KEY_FILE_NAME, MEDIA_DIR_NAME, OVERLAYS_DIR_NAME, TTS_DIR_NAME,
    credentials_key_path_in,
};

const ARCHIVE_ROOT: &str = "forge";
const ARCHIVE_EXTENSION: &str = "tar.gz";
const PARTIAL_EXTENSION: &str = "partial";
const SNAPSHOT_EXTENSION: &str = "db.snapshot";
const FILE_PREFIX: &str = "forge-backup";
const SUFFIX_LEN: usize = 6;

const INCLUDED_DIRS: [&str; 4] = [
    MEDIA_DIR_NAME,
    OVERLAYS_DIR_NAME,
    TTS_DIR_NAME,
    ASSETS_DIR_NAME,
];

#[cfg(unix)]
const OWNER_ONLY_MODE: u32 = 0o600;

pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUPS_DIR_NAME)
}

pub fn backup_file_stem(now: OffsetDateTime, suffix: &str) -> String {
    format!(
        "{FILE_PREFIX}-{:04}{:02}{:02}-{:02}{:02}{:02}Z-{suffix}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}

pub fn unique_suffix() -> String {
    let id = ulid::Ulid::generate().to_string().to_lowercase();
    id.split_at(id.len().saturating_sub(SUFFIX_LEN))
        .1
        .to_owned()
}

pub async fn run_backup(
    backend: Arc<dyn DataProvider>,
    data_dir: PathBuf,
) -> Result<PathBuf, String> {
    let dir = backups_dir(&data_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let stem = backup_file_stem(OffsetDateTime::now_utc(), &unique_suffix());
    let snapshot = dir.join(format!("{stem}.{SNAPSHOT_EXTENSION}"));
    let archive = dir.join(format!("{stem}.{ARCHIVE_EXTENSION}"));
    let partial = dir.join(format!("{stem}.{ARCHIVE_EXTENSION}.{PARTIAL_EXTENSION}"));

    let exported = backend.export(&snapshot).await;
    let result = match exported {
        Ok(()) => {
            let (snapshot_in, partial_in, archive_in) =
                (snapshot.clone(), partial.clone(), archive.clone());
            tokio::task::spawn_blocking(move || {
                write_archive(&data_dir, &snapshot_in, &partial_in)?;
                std::fs::rename(&partial_in, &archive_in)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|written| written.map_err(|e| e.to_string()))
        }
        Err(e) => Err(format!("database snapshot failed: {e}")),
    };

    let _ = tokio::fs::remove_file(&snapshot).await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&partial).await;
    }
    result.map(|()| archive)
}

pub fn write_archive(data_dir: &Path, db_snapshot: &Path, dest: &Path) -> io::Result<()> {
    let file = create_private(dest)?;
    let encoder = GzEncoder::new(BufWriter::new(file), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);

    let root = Path::new(ARCHIVE_ROOT);
    builder.append_path_with_name(db_snapshot, root.join(DATABASE_FILE_NAME))?;

    let key = credentials_key_path_in(data_dir);
    if key.is_file() {
        builder.append_path_with_name(&key, root.join(CREDENTIALS_KEY_FILE_NAME))?;
    }

    for name in INCLUDED_DIRS {
        let src = data_dir.join(name);
        if src.is_dir() {
            builder.append_dir_all(root.join(name), &src)?;
        }
    }

    let mut writer = builder.into_inner()?.finish()?;
    writer.flush()?;
    writer.get_ref().sync_all()
}

#[cfg(unix)]
fn create_private(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY_MODE)
        .open(path)
}

#[cfg(not(unix))]
fn create_private(path: &Path) -> io::Result<File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::io::Read;

    use super::*;

    const CREDENTIALS_KEY_FILE_ENV: &str = "FORGE_CREDENTIAL_KEY_FILE";

    static CREDENTIALS_KEY_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[allow(unsafe_code)]
    fn with_credentials_key_override<R>(key_file: &Path, body: impl FnOnce() -> R) -> R {
        let _guard = CREDENTIALS_KEY_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::var_os(CREDENTIALS_KEY_FILE_ENV);
        // SAFETY: CREDENTIALS_KEY_ENV_LOCK serialises every test in this binary that touches this variable, and no other test in it reads it.
        unsafe {
            std::env::set_var(CREDENTIALS_KEY_FILE_ENV, key_file);
        }
        let result = body();
        // SAFETY: still under CREDENTIALS_KEY_ENV_LOCK, restoring the value seen on entry.
        unsafe {
            match previous {
                Some(v) => std::env::set_var(CREDENTIALS_KEY_FILE_ENV, v),
                None => std::env::remove_var(CREDENTIALS_KEY_FILE_ENV),
            }
        }
        result
    }

    fn archived_file(archive: &Path, name: &Path) -> Option<Vec<u8>> {
        let decoder = flate2::read::GzDecoder::new(File::open(archive).unwrap());
        let mut entries = tar::Archive::new(decoder);
        for entry in entries.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.path().unwrap() == name {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                return Some(bytes);
            }
        }
        None
    }

    #[test]
    fn write_archive_packs_the_overridden_credentials_key_instead_of_the_data_dir_one() {
        let data_dir = tempfile::tempdir().unwrap();
        std::fs::write(data_dir.path().join(CREDENTIALS_KEY_FILE_NAME), b"stale").unwrap();
        let key_dir = tempfile::tempdir().unwrap();
        let key_file = key_dir.path().join("custom.key");
        std::fs::write(&key_file, b"live key bytes").unwrap();
        let snapshot = data_dir.path().join("snapshot.db");
        std::fs::write(&snapshot, b"db").unwrap();
        let archive = data_dir.path().join("backup.tar.gz");

        with_credentials_key_override(&key_file, || {
            write_archive(data_dir.path(), &snapshot, &archive)
        })
        .unwrap();

        let packed = archived_file(
            &archive,
            &Path::new(ARCHIVE_ROOT).join(CREDENTIALS_KEY_FILE_NAME),
        );
        assert_eq!(packed.as_deref(), Some(&b"live key bytes"[..]));
    }
}
