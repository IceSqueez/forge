use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flate2::Compression;
use flate2::write::GzEncoder;
use forge_storage::DataProvider;
use time::OffsetDateTime;

pub const BACKUPS_DIR_NAME: &str = "backups";

const ARCHIVE_ROOT: &str = "forge";
const ARCHIVE_EXTENSION: &str = "tar.gz";
const PARTIAL_EXTENSION: &str = "partial";
const SNAPSHOT_EXTENSION: &str = "db.snapshot";
const FILE_PREFIX: &str = "forge-backup";
const SUFFIX_LEN: usize = 6;

const DB_ENTRY: &str = "forge.db";
const KEY_FILE: &str = "credentials-key";
const INCLUDED_DIRS: [&str; 4] = ["media", "overlays", "tts", "assets"];

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

/// The DB enters as an `export` snapshot, never the live file; the snapshot and any partial archive are removed on every path.
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

/// Allowlist, not a walk of `data_dir`: WAL/SHM, the instance lock, logs and earlier backups stay out.
pub fn write_archive(data_dir: &Path, db_snapshot: &Path, dest: &Path) -> io::Result<()> {
    let file = create_private(dest)?;
    let encoder = GzEncoder::new(BufWriter::new(file), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.follow_symlinks(false);

    let root = Path::new(ARCHIVE_ROOT);
    builder.append_path_with_name(db_snapshot, root.join(DB_ENTRY))?;

    let key = data_dir.join(KEY_FILE);
    if key.is_file() {
        builder.append_path_with_name(&key, root.join(KEY_FILE))?;
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
