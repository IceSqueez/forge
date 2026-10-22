use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use forge_storage::SettingsRepo;
use time::OffsetDateTime;

use crate::durable_fs::{exists, remove_dir_all_if_present, remove_file_if_present};
use crate::error::TransferError;
use crate::exclusivity::TransferExclusivity;
use crate::safety_copies::inventory::{SafetyCopy, inventory};
use crate::safety_copies::kind::SafetyCopyKind;
use crate::safety_copies::protection::protection_for;
use crate::safety_copies::selection::{doomed, everything_unprotected};
use crate::safety_copies::settings::SafetyCopySettings;
use crate::snapshot::run_blocking;

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
        let removed = if copy.kind == SafetyCopyKind::PreRestore {
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
