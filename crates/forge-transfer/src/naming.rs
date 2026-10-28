use std::path::{Path, PathBuf};

use forge_paths::BACKUPS_DIR_NAME;
use time::OffsetDateTime;

pub const BUNDLE_EXTENSION: &str = "forgebundle";
pub const LEGACY_ARCHIVE_EXTENSION: &str = "tar.gz";
pub const PRE_UPDATE_SNAPSHOT_EXTENSION: &str = "pre-update.db";

pub(crate) const FILE_PREFIX: &str = "forge-backup";
pub(crate) const SCHEDULED_PREFIX: &str = "forge-scheduled";
pub(crate) const PRE_RESTORE_PREFIX: &str = "forge-pre-restore";
pub(crate) const PRE_PACK_PREFIX: &str = "forge-pre-pack";
pub(crate) const PRE_SETUP_PREFIX: &str = "forge-pre-setup";
const SUFFIX_LEN: usize = 6;
pub(crate) const PARTIAL_EXTENSION: &str = "partial";
pub(crate) const CIPHERTEXT_EXTENSION: &str = "payload.partial";
const SNAPSHOT_EXTENSION: &str = "db.snapshot";

pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUPS_DIR_NAME)
}

pub fn backup_file_stem(now: OffsetDateTime, suffix: &str) -> String {
    stamped_stem(FILE_PREFIX, now, suffix)
}

fn stamped_stem(prefix: &str, now: OffsetDateTime, suffix: &str) -> String {
    format!(
        "{prefix}-{:04}{:02}{:02}-{:02}{:02}{:02}Z-{suffix}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
    )
}

const STAMP_FORMAT: &str = "[year][month][day]-[hour][minute][second]";
const STAMP_ZONE_MARK: char = 'Z';

pub(crate) fn stamp_of(stem_after_prefix: &str) -> Option<OffsetDateTime> {
    let (stamp, _) = stem_after_prefix.split_once(STAMP_ZONE_MARK)?;
    let format = time::format_description::parse_borrowed::<2>(STAMP_FORMAT).ok()?;
    time::PrimitiveDateTime::parse(stamp, &format)
        .ok()
        .map(time::PrimitiveDateTime::assume_utc)
}

pub fn unique_suffix() -> String {
    let id = ulid::Ulid::generate().to_string().to_lowercase();
    id.split_at(id.len().saturating_sub(SUFFIX_LEN))
        .1
        .to_owned()
}

pub(crate) fn fresh_scheduled_stem() -> String {
    stamped_stem(
        SCHEDULED_PREFIX,
        OffsetDateTime::now_utc(),
        &unique_suffix(),
    )
}

pub(crate) fn fresh_stem() -> String {
    backup_file_stem(OffsetDateTime::now_utc(), &unique_suffix())
}

pub(crate) fn fresh_pre_restore_dir(data_dir: &Path) -> PathBuf {
    backups_dir(data_dir).join(stamped_stem(
        PRE_RESTORE_PREFIX,
        OffsetDateTime::now_utc(),
        &unique_suffix(),
    ))
}

pub(crate) fn fresh_pre_pack_stem() -> String {
    stamped_stem(PRE_PACK_PREFIX, OffsetDateTime::now_utc(), &unique_suffix())
}

pub(crate) fn fresh_pre_setup_stem() -> String {
    stamped_stem(
        PRE_SETUP_PREFIX,
        OffsetDateTime::now_utc(),
        &unique_suffix(),
    )
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
