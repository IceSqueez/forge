use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use forge_storage::SettingsRepo;
use time::OffsetDateTime;

use crate::bundle::blocking::run_blocking;
use crate::bundle::durable_fs::{exists, remove_dir_all_if_present, remove_file_if_present};
use crate::bundle::exclusivity::TransferExclusivity;
use crate::error::TransferError;
use crate::safety_copies::inventory::{SafetyCopy, inventory};
use crate::safety_copies::kind::SafetyCopyKind;
use crate::safety_copies::protection::protection_for;
use crate::safety_copies::selection::{doomed, everything_unprotected};
use crate::safety_copies::settings::SafetyCopySettings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyCopyFailure {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SafetyCopyReport {
    pub removed: u32,
    pub freed_bytes: u64,
    pub failures: Vec<SafetyCopyFailure>,
    pub deferred_by_transfer: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SafetyCopyUsage {
    pub count: u32,
    pub bytes: u64,
}

enum Sweep {
    ByPolicy(SafetyCopySettings),
    Everything,
}

pub async fn safety_copy_usage(data_dir: &Path) -> Result<SafetyCopyUsage, TransferError> {
    run_blocking(data_dir.to_path_buf(), |data_dir| {
        let copies = inventory(&data_dir)?;
        Ok(SafetyCopyUsage {
            count: u32::try_from(copies.len()).unwrap_or(u32::MAX),
            bytes: copies.iter().map(|copy| copy.bytes).sum(),
        })
    })
    .await
}

pub async fn prune_safety_copies(
    settings: &dyn SettingsRepo,
    data_dir: &Path,
    exclusivity: &TransferExclusivity,
) -> Result<SafetyCopyReport, TransferError> {
    let policy = SafetyCopySettings::load(settings).await?;
    sweep_exclusively(data_dir, exclusivity, Sweep::ByPolicy(policy)).await
}

pub async fn clean_up_safety_copies(
    data_dir: &Path,
    exclusivity: &TransferExclusivity,
) -> Result<SafetyCopyReport, TransferError> {
    sweep_exclusively(data_dir, exclusivity, Sweep::Everything).await
}

pub async fn change_safety_copy_settings(
    settings: &dyn SettingsRepo,
    data_dir: &Path,
    exclusivity: &TransferExclusivity,
    changed: SafetyCopySettings,
) -> Result<SafetyCopyReport, TransferError> {
    changed.save(settings).await?;
    prune_safety_copies(settings, data_dir, exclusivity).await
}

pub(crate) async fn prune_after_safety_copy(
    settings: &dyn SettingsRepo,
    data_dir: &Path,
    just_written: &Path,
) {
    let swept = match SafetyCopySettings::load(settings).await {
        Ok(policy) => sweep(data_dir, Sweep::ByPolicy(policy), just_written.file_name()).await,
        Err(e) => Err(e.into()),
    };
    match swept {
        Ok(report) => {
            for failure in &report.failures {
                tracing::warn!(
                    path = %failure.path.display(),
                    reason = %failure.reason,
                    "could not delete a safety copy"
                );
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not prune safety copies"),
    }
}

async fn sweep_exclusively(
    data_dir: &Path,
    exclusivity: &TransferExclusivity,
    plan: Sweep,
) -> Result<SafetyCopyReport, TransferError> {
    let Some(_guard) = exclusivity.try_acquire() else {
        return Ok(SafetyCopyReport {
            deferred_by_transfer: true,
            ..SafetyCopyReport::default()
        });
    };
    sweep(data_dir, plan, None).await
}

async fn sweep(
    data_dir: &Path,
    plan: Sweep,
    spared: Option<&OsStr>,
) -> Result<SafetyCopyReport, TransferError> {
    let spared = spared.map(OsStr::to_owned);
    let now = OffsetDateTime::now_utc();
    run_blocking(data_dir.to_path_buf(), move |data_dir| {
        let copies = inventory(&data_dir)?;
        let mut protection = protection_for(&data_dir);
        protection.names.extend(spared);
        let condemned = match plan {
            Sweep::ByPolicy(policy) => doomed(copies, policy.retention(), now, &protection),
            Sweep::Everything => everything_unprotected(copies, &protection),
        };
        Ok(remove_all(condemned))
    })
    .await
}

fn remove_all(condemned: Vec<SafetyCopy>) -> SafetyCopyReport {
    let mut report = SafetyCopyReport::default();
    for copy in condemned {
        let existed = exists(&copy.path).unwrap_or(true);
        let removed = if copy.kind == SafetyCopyKind::Restore {
            remove_dir_all_if_present(&copy.path)
        } else {
            remove_file_if_present(&copy.path)
        };
        match removed {
            Ok(()) => {
                report.removed += 1;
                if existed {
                    report.freed_bytes += copy.bytes;
                }
            }
            Err(e) => report.failures.push(SafetyCopyFailure {
                path: copy.path,
                reason: e.to_string(),
            }),
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use time::OffsetDateTime;

    use forge_storage::DATABASE_FILE_NAME;
    use forge_storage_sqlite::SqliteBackend;
    use time::Duration;

    use super::{prune_after_safety_copy, remove_all};
    use crate::bundle::naming::{
        FILE_PREFIX, LEGACY_ARCHIVE_EXTENSION, PRE_PACK_PREFIX, backup_file_stem, backups_dir,
    };
    use crate::safety_copies::inventory::SafetyCopy;
    use crate::safety_copies::kind::SafetyCopyKind;
    use crate::safety_copies::settings::{SafetyCopyMode, SafetyCopySettings};

    #[cfg(unix)]
    const READ_AND_LIST_ONLY: u32 = 0o500;
    #[cfg(unix)]
    const OWNER_FULL_ACCESS: u32 = 0o700;

    fn copy(path: &Path, kind: SafetyCopyKind) -> SafetyCopy {
        SafetyCopy {
            path: path.to_path_buf(),
            kind,
            created_at: OffsetDateTime::UNIX_EPOCH,
            bytes: 1,
        }
    }

    #[test]
    fn a_copy_that_is_already_gone_counts_as_removed() {
        let dir = tempfile::tempdir().unwrap();

        let report = remove_all(vec![
            copy(&dir.path().join("vanished-dir"), SafetyCopyKind::Restore),
            copy(&dir.path().join("vanished.tar.gz"), SafetyCopyKind::Pack),
        ]);

        assert_eq!((report.removed, report.failures.len()), (2, 0));
    }

    #[test]
    fn freed_bytes_count_only_copies_that_still_existed() {
        let dir = tempfile::tempdir().unwrap();
        let present = dir.path().join("present.tar.gz");
        std::fs::write(&present, b"x").unwrap();

        let report = remove_all(vec![
            copy(&present, SafetyCopyKind::Pack),
            copy(&dir.path().join("vanished.tar.gz"), SafetyCopyKind::Pack),
        ]);

        assert_eq!((report.removed, report.freed_bytes), (2, 1));
    }

    #[tokio::test]
    async fn the_sweep_after_a_safety_copy_never_deletes_the_copy_it_just_wrote() {
        let data = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}",
            data.path().join(DATABASE_FILE_NAME).display()
        );
        let repo = SqliteBackend::open_for_test(&url, [7; 32], data.path().join("media"), None)
            .await
            .unwrap();
        SafetyCopySettings {
            mode: SafetyCopyMode::KeepNewest,
            keep_newest: 1,
            delete_after_days: 1,
        }
        .save(&repo)
        .await
        .unwrap();
        let backups = backups_dir(data.path());
        std::fs::create_dir_all(&backups).unwrap();
        let pre_pack = |at: OffsetDateTime, suffix: &str| {
            let stem = backup_file_stem(at, suffix);
            let path = backups.join(format!(
                "{PRE_PACK_PREFIX}{}.{LEGACY_ARCHIVE_EXTENSION}",
                stem.strip_prefix(FILE_PREFIX).unwrap()
            ));
            std::fs::write(&path, b"x").unwrap();
            path
        };
        let now = OffsetDateTime::now_utc();
        let skewed_ahead = pre_pack(now + Duration::days(2), "ahead1");
        let older = pre_pack(now - Duration::days(2), "older1");
        let just_written = pre_pack(now, "fresh1");

        prune_after_safety_copy(&repo, data.path(), &just_written).await;

        assert_eq!(
            (just_written.exists(), skewed_ahead.exists(), older.exists()),
            (true, true, false)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_that_cannot_be_deleted_is_reported_and_the_rest_are_still_removed() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let stuck = dir.path().join("stuck");
        let locked = stuck.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("forge.db"), b"x").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(READ_AND_LIST_ONLY))
            .unwrap();
        let loose = dir.path().join("loose.tar.gz");
        std::fs::write(&loose, b"x").unwrap();

        let report = remove_all(vec![
            copy(&stuck, SafetyCopyKind::Restore),
            copy(&loose, SafetyCopyKind::Pack),
        ]);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(OWNER_FULL_ACCESS))
            .unwrap();

        assert_eq!(
            (
                report.removed,
                report
                    .failures
                    .iter()
                    .map(|f| f.path.clone())
                    .collect::<Vec<_>>(),
                loose.exists(),
                stuck.exists()
            ),
            (1, vec![stuck.clone()], false, true)
        );
    }
}
