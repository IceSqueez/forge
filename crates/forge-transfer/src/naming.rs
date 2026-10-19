use std::path::{Path, PathBuf};

use forge_storage::paths::BACKUPS_DIR_NAME;
use time::OffsetDateTime;

pub const BUNDLE_EXTENSION: &str = "forgebundle";
pub const LEGACY_ARCHIVE_EXTENSION: &str = "tar.gz";

const FILE_PREFIX: &str = "forge-backup";
const SUFFIX_LEN: usize = 6;
const PARTIAL_EXTENSION: &str = "partial";
const CIPHERTEXT_EXTENSION: &str = "payload.partial";
const SNAPSHOT_EXTENSION: &str = "db.snapshot";

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

pub(crate) fn fresh_stem() -> String {
    backup_file_stem(OffsetDateTime::now_utc(), &unique_suffix())
}

pub(crate) fn snapshot_path(data_dir: &Path, stem: &str) -> PathBuf {
    backups_dir(data_dir).join(format!("{stem}.{SNAPSHOT_EXTENSION}"))
}

pub(crate) fn sibling_with_extension(path: &Path, extension: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(extension);
    path.with_file_name(name)
}

pub(crate) fn partial_path(destination: &Path) -> PathBuf {
    sibling_with_extension(destination, PARTIAL_EXTENSION)
}

pub(crate) fn ciphertext_path(destination: &Path) -> PathBuf {
    sibling_with_extension(destination, CIPHERTEXT_EXTENSION)
}
