use std::ops::RangeInclusive;
use std::sync::Arc;

use forge_components::{DateShape, fmt_display, fmt_display_relative, tr};
use forge_storage::settings::core_settings::{
    BACKUP_SCHEDULE_DESTINATION, BACKUP_SCHEDULE_ENABLED, BACKUP_SCHEDULE_INCLUDE_ASSETS,
    BACKUP_SCHEDULE_INCLUDE_MEDIA, BACKUP_SCHEDULE_INCLUDE_OVERLAYS,
    BACKUP_SCHEDULE_INCLUDE_VOICES, BACKUP_SCHEDULE_INTERVAL_MINUTES,
    BACKUP_SCHEDULE_KEEP_DAILY_DAYS, BACKUP_SCHEDULE_KEEP_NEWEST, BACKUP_SCHEDULE_KEY,
};
use forge_storage::settings::{
    DEFAULT_BACKUP_SCHEDULE_ENABLED, DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY,
    DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES, DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
    DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST, MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES,
    MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS, MAX_BACKUP_SCHEDULE_KEEP_NEWEST,
    MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES, MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
    MIN_BACKUP_SCHEDULE_KEEP_NEWEST,
};
use forge_storage::{
    Count, Flag, OptionalText, Setting, SettingsRepo, StorageError, read_setting,
    read_stored_setting, write_setting,
};
use forge_transfer::{
    BackupCategory, FailureCategory, Passphrase, ScheduleKey, ScheduleState, ScheduledBackupStatus,
    StorageUsed, TransferError, change_schedule_passphrase, create_schedule_key,
};
use time::OffsetDateTime;

use crate::data_flows::restore_problem::RestoreProblem;
use crate::shared::repo_failure::RepoFailure;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScheduleNumber {
    Interval,
    KeepNewest,
    KeepDailyDays,
}

impl ScheduleNumber {
    pub(crate) const ALL: [Self; 3] = [Self::Interval, Self::KeepNewest, Self::KeepDailyDays];

    pub(super) fn declaration(self) -> &'static Setting<Count> {
        match self {
            Self::Interval => &BACKUP_SCHEDULE_INTERVAL_MINUTES,
            Self::KeepNewest => &BACKUP_SCHEDULE_KEEP_NEWEST,
            Self::KeepDailyDays => &BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        }
    }

    pub(crate) fn storage_key(self) -> &'static str {
        self.declaration().name()
    }

    pub(crate) fn range(self) -> RangeInclusive<u32> {
        match self {
            Self::Interval => {
                MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES..=MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES
            }
            Self::KeepNewest => MIN_BACKUP_SCHEDULE_KEEP_NEWEST..=MAX_BACKUP_SCHEDULE_KEEP_NEWEST,
            Self::KeepDailyDays => {
                MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS..=MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS
            }
        }
    }

    pub(crate) fn default_value(self) -> u32 {
        match self {
            Self::Interval => DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES,
            Self::KeepNewest => DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST,
            Self::KeepDailyDays => DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
        }
    }

    pub(crate) fn parse(self, text: &str) -> Option<u32> {
        text.trim()
            .parse::<u32>()
            .ok()
            .filter(|value| self.range().contains(value))
    }

    pub(super) fn label(self) -> String {
        match self {
            Self::Interval => tr!("backup_schedule_interval_label"),
            Self::KeepNewest => tr!("backup_schedule_keep_newest_label"),
            Self::KeepDailyDays => tr!("backup_schedule_keep_daily_label"),
        }
    }

    pub(super) fn hint(self) -> String {
        let range = self.range();
        let (min, max) = (*range.start(), *range.end());
        match self {
            Self::Interval => tr!("backup_schedule_interval_hint", min = min, max = max),
            Self::KeepNewest => tr!("backup_schedule_keep_newest_hint", min = min, max = max),
            Self::KeepDailyDays => tr!("backup_schedule_keep_daily_hint", min = min, max = max),
        }
    }

    pub(super) fn range_error(self) -> String {
        let range = self.range();
        tr!(
            "backup_schedule_range_error",
            min = *range.start(),
            max = *range.end()
        )
    }

    pub(super) fn unit(self, count: u32) -> String {
        match self {
            Self::Interval => tr!("backup_schedule_unit_minutes", count = count),
            Self::KeepNewest => tr!("backup_schedule_unit_backups", count = count),
            Self::KeepDailyDays => tr!("backup_schedule_unit_days", count = count),
        }
    }
}

pub(crate) const OPTIONAL_CATEGORIES: [BackupCategory; 4] = [
    BackupCategory::Media,
    BackupCategory::Voices,
    BackupCategory::Overlays,
    BackupCategory::Assets,
];

pub(crate) fn include_setting(category: BackupCategory) -> Option<&'static Setting<Flag>> {
    match category {
        BackupCategory::Database => None,
        BackupCategory::Media => Some(&BACKUP_SCHEDULE_INCLUDE_MEDIA),
        BackupCategory::Voices => Some(&BACKUP_SCHEDULE_INCLUDE_VOICES),
        BackupCategory::Overlays => Some(&BACKUP_SCHEDULE_INCLUDE_OVERLAYS),
        BackupCategory::Assets => Some(&BACKUP_SCHEDULE_INCLUDE_ASSETS),
    }
}

pub(crate) fn category_label(category: BackupCategory) -> String {
    match category {
        BackupCategory::Database => tr!("backup_schedule_category_database"),
        BackupCategory::Media => tr!("backup_schedule_category_media"),
        BackupCategory::Voices => tr!("backup_schedule_category_voices"),
        BackupCategory::Overlays => tr!("backup_schedule_category_overlays"),
        BackupCategory::Assets => tr!("backup_schedule_category_assets"),
    }
}

pub(crate) fn failure_reason(category: FailureCategory) -> String {
    match category {
        FailureCategory::DestinationMissing => tr!("backup_schedule_failure_destination_missing"),
        FailureCategory::DestinationNotWritable => {
            tr!("backup_schedule_failure_destination_not_writable")
        }
        FailureCategory::DiskFull => tr!("backup_schedule_failure_disk_full"),
        FailureCategory::DestinationInsideBackupScope => {
            tr!("backup_schedule_failure_inside_scope")
        }
        FailureCategory::KeyMissing => tr!("backup_schedule_failure_key_missing"),
        FailureCategory::KeyUnreadable => tr!("backup_schedule_failure_key_unreadable"),
        FailureCategory::SnapshotFailed => tr!("backup_schedule_failure_snapshot_failed"),
        FailureCategory::StateNotSaved => tr!("backup_schedule_failure_state_not_saved"),
        FailureCategory::Interrupted => tr!("backup_schedule_failure_interrupted"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoragePart {
    Database,
    Media,
    Voices,
    Overlays,
    Assets,
    Logs,
    Backups,
    Destination,
}

impl StoragePart {
    pub(super) fn label(self) -> String {
        match self {
            Self::Database => tr!("backup_schedule_storage_used_database"),
            Self::Media => tr!("backup_schedule_storage_used_media"),
            Self::Voices => tr!("backup_schedule_storage_used_voices"),
            Self::Overlays => tr!("backup_schedule_storage_used_overlays"),
            Self::Assets => tr!("backup_schedule_storage_used_assets"),
            Self::Logs => tr!("backup_schedule_storage_used_logs"),
            Self::Backups => tr!("backup_schedule_storage_used_backups"),
            Self::Destination => tr!("backup_schedule_storage_used_destination"),
        }
    }
}

pub(crate) fn storage_parts(used: &StorageUsed) -> Vec<(StoragePart, u64)> {
    let mut parts = vec![
        (StoragePart::Database, used.database),
        (StoragePart::Media, used.media),
        (StoragePart::Voices, used.voices),
        (StoragePart::Overlays, used.overlays),
        (StoragePart::Assets, used.assets),
        (StoragePart::Logs, used.logs),
        (StoragePart::Backups, used.backups),
    ];
    if used.destination_outside_data_dir > 0 {
        parts.push((StoragePart::Destination, used.destination_outside_data_dir));
    }
    parts
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLine {
    Off {
        last_success_at: Option<OffsetDateTime>,
    },
    Paused,
    Running,
    Failing {
        category: FailureCategory,
        since: OffsetDateTime,
        attempts: u32,
    },
    Scheduled {
        last_success_at: Option<OffsetDateTime>,
        next_due: Option<OffsetDateTime>,
    },
}

impl StatusLine {
    pub(crate) fn of(status: &ScheduledBackupStatus) -> Self {
        match (status.state, status.failure) {
            (ScheduleState::Disabled, _) => Self::Off {
                last_success_at: status.last_success_at,
            },
            (ScheduleState::Suspended, _) => Self::Paused,
            (ScheduleState::Running, _) => Self::Running,
            (ScheduleState::Idle, Some(streak)) => Self::Failing {
                category: streak.category,
                since: streak.since,
                attempts: streak.attempts,
            },
            (ScheduleState::Idle, None) => Self::Scheduled {
                last_success_at: status.last_success_at,
                next_due: status.next_due,
            },
        }
    }

    pub(crate) fn is_failure(self) -> bool {
        matches!(self, Self::Failing { .. })
    }

    pub(super) fn text(self) -> String {
        match self {
            Self::Off { last_success_at } => tr!(
                "backup_schedule_status_off",
                last = fmt_display_relative(last_success_at)
            ),
            Self::Paused => tr!("backup_schedule_status_paused"),
            Self::Running => tr!("backup_schedule_status_running"),
            Self::Failing {
                category,
                since,
                attempts,
            } => tr!(
                "backup_schedule_status_failing",
                since = fmt_display(DateShape::DateTime, since),
                attempts = attempts,
                reason = failure_reason(category)
            ),
            Self::Scheduled {
                last_success_at,
                next_due: Some(next),
            } => tr!(
                "backup_schedule_status_scheduled",
                last = fmt_display_relative(last_success_at),
                next = fmt_display(DateShape::DateTime, next)
            ),
            Self::Scheduled {
                last_success_at,
                next_due: None,
            } => tr!(
                "backup_schedule_status_last",
                last = fmt_display_relative(last_success_at)
            ),
        }
    }
}

pub(crate) struct ScheduleSettings {
    pub enabled: bool,
    pub key: Option<ScheduleKey>,
    pub included: Vec<BackupCategory>,
    pub destination: Option<String>,
}

pub(crate) struct ScheduleSnapshot {
    pub settings: ScheduleSettings,
    pub numbers: Vec<(ScheduleNumber, u32)>,
}

pub(super) async fn stored_text(
    repo: &dyn SettingsRepo,
    setting: &Setting<OptionalText>,
) -> Result<Option<String>, StorageError> {
    Ok(read_setting(repo, setting)
        .await?
        .filter(|value| !value.trim().is_empty()))
}

pub(crate) async fn load_schedule(
    repo: Arc<dyn SettingsRepo>,
) -> Result<ScheduleSnapshot, StorageError> {
    let repo = repo.as_ref();
    let mut numbers = Vec::with_capacity(ScheduleNumber::ALL.len());
    for number in ScheduleNumber::ALL {
        let stored = read_stored_setting(repo, number.declaration()).await?;
        numbers.push((number, stored.unwrap_or_else(|| number.default_value())));
    }
    let mut included = Vec::with_capacity(OPTIONAL_CATEGORIES.len());
    for category in OPTIONAL_CATEGORIES {
        if let Some(setting) = include_setting(category)
            && read_stored_setting(repo, setting)
                .await?
                .unwrap_or(DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY)
        {
            included.push(category);
        }
    }
    let key = stored_text(repo, &BACKUP_SCHEDULE_KEY)
        .await?
        .and_then(|stored| ScheduleKey::decode(&stored).ok());
    let enabled = read_stored_setting(repo, &BACKUP_SCHEDULE_ENABLED)
        .await?
        .unwrap_or(DEFAULT_BACKUP_SCHEDULE_ENABLED);
    Ok(ScheduleSnapshot {
        settings: ScheduleSettings {
            enabled,
            key,
            included,
            destination: stored_text(repo, &BACKUP_SCHEDULE_DESTINATION).await?,
        },
        numbers,
    })
}

#[derive(Debug)]
pub(crate) enum KeyFailure {
    WrongCurrent,
    Transfer(RestoreProblem),
    Failed(RepoFailure),
}

impl KeyFailure {
    pub(super) fn text(&self) -> String {
        match self {
            Self::WrongCurrent => RestoreProblem::WrongPassphrase.message(),
            Self::Transfer(problem) => problem.plain_text(),
            Self::Failed(failure) => failure.text(),
        }
    }
}

impl From<TransferError> for KeyFailure {
    fn from(error: TransferError) -> Self {
        match error {
            TransferError::WrongPassphrase => Self::WrongCurrent,
            other => Self::Transfer(RestoreProblem::of(&other)),
        }
    }
}

impl From<StorageError> for KeyFailure {
    fn from(error: StorageError) -> Self {
        Self::Failed(error.into())
    }
}

pub(super) async fn store_key(
    repo: &dyn SettingsRepo,
    key: &ScheduleKey,
    enable: bool,
) -> Result<(), KeyFailure> {
    write_setting(repo, &BACKUP_SCHEDULE_KEY, &Some(key.encode()?)).await?;
    if enable {
        write_setting(repo, &BACKUP_SCHEDULE_ENABLED, &true).await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManualRunProgress {
    Pending,
    Succeeded,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ManualRunWatch {
    baseline: Option<OffsetDateTime>,
    started: bool,
}

impl ManualRunWatch {
    pub(crate) fn since(last_success_at: Option<OffsetDateTime>) -> Self {
        Self {
            baseline: last_success_at,
            started: false,
        }
    }

    pub(crate) fn observe(&mut self, status: &ScheduledBackupStatus) -> ManualRunProgress {
        if status.last_success_at > self.baseline {
            ManualRunProgress::Succeeded
        } else if status.state == ScheduleState::Running {
            self.started = true;
            ManualRunProgress::Pending
        } else if self.started {
            ManualRunProgress::Ended
        } else {
            ManualRunProgress::Pending
        }
    }
}

pub(super) enum KeyWork {
    Create(Passphrase),
    Change {
        wrapped: String,
        current: Passphrase,
        replacement: Passphrase,
    },
}

impl KeyWork {
    pub(super) fn run(self) -> Result<ScheduleKey, KeyFailure> {
        match self {
            Self::Create(passphrase) => Ok(create_schedule_key(&passphrase)?),
            Self::Change {
                wrapped,
                current,
                replacement,
            } => Ok(change_schedule_passphrase(
                &wrapped,
                &current,
                &replacement,
            )?),
        }
    }
}
