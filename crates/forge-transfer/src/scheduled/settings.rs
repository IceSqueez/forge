use std::path::PathBuf;

use forge_storage::settings::{
    DEFAULT_BACKUP_SCHEDULE_ENABLED, DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY,
    DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES, DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
    DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST, MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES,
    MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS, MAX_BACKUP_SCHEDULE_KEEP_NEWEST,
    MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES, MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
    MIN_BACKUP_SCHEDULE_KEEP_NEWEST,
};
use forge_storage::{SettingsRepo, StorageError, get_bool_setting, reserved_keys};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::category::BackupCategory;
use crate::schedule_key::ScheduleKey;
use crate::scheduled::retention::RetentionPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScheduleSettings {
    pub enabled: bool,
    pub interval_minutes: u32,
    pub retention: RetentionPolicy,
    pub categories: Vec<BackupCategory>,
    pub destination: Option<PathBuf>,
    pub key: Option<ScheduleKey>,
}

impl Default for ScheduleSettings {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_BACKUP_SCHEDULE_ENABLED,
            interval_minutes: DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES,
            retention: RetentionPolicy {
                keep_newest: DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST,
                keep_daily_days: DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
            },
            categories: BackupCategory::all(),
            destination: None,
            key: None,
        }
    }
}

impl ScheduleSettings {
    pub(crate) fn interval(&self) -> Duration {
        Duration::minutes(i64::from(self.interval_minutes))
    }

    pub(crate) async fn load(repo: &dyn SettingsRepo) -> Result<Self, StorageError> {
        let include = |key: &'static str, category: BackupCategory| async move {
            get_bool_setting(repo, key, DEFAULT_BACKUP_SCHEDULE_INCLUDE_CATEGORY)
                .await
                .then_some(category)
        };
        let optional_categories = [
            include(
                reserved_keys::BACKUP_SCHEDULE_INCLUDE_MEDIA,
                BackupCategory::Media,
            )
            .await,
            include(
                reserved_keys::BACKUP_SCHEDULE_INCLUDE_VOICES,
                BackupCategory::Voices,
            )
            .await,
            include(
                reserved_keys::BACKUP_SCHEDULE_INCLUDE_OVERLAYS,
                BackupCategory::Overlays,
            )
            .await,
            include(
                reserved_keys::BACKUP_SCHEDULE_INCLUDE_ASSETS,
                BackupCategory::Assets,
            )
            .await,
        ];
        let categories = std::iter::once(BackupCategory::Database)
            .chain(optional_categories.into_iter().flatten())
            .collect();
        let recipient = text(repo, reserved_keys::BACKUP_SCHEDULE_RECIPIENT).await?;
        let wrapped_identity = text(repo, reserved_keys::BACKUP_SCHEDULE_WRAPPED_IDENTITY).await?;
        Ok(Self {
            enabled: get_bool_setting(
                repo,
                reserved_keys::BACKUP_SCHEDULE_ENABLED,
                DEFAULT_BACKUP_SCHEDULE_ENABLED,
            )
            .await,
            interval_minutes: bounded(
                repo,
                reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES,
                (
                    MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES,
                    DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES,
                    MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES,
                ),
            )
            .await?,
            retention: RetentionPolicy {
                keep_newest: bounded(
                    repo,
                    reserved_keys::BACKUP_SCHEDULE_KEEP_NEWEST,
                    (
                        MIN_BACKUP_SCHEDULE_KEEP_NEWEST,
                        DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST,
                        MAX_BACKUP_SCHEDULE_KEEP_NEWEST,
                    ),
                )
                .await?,
                keep_daily_days: bounded(
                    repo,
                    reserved_keys::BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
                    (
                        MIN_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
                        DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
                        MAX_BACKUP_SCHEDULE_KEEP_DAILY_DAYS,
                    ),
                )
                .await?,
            },
            categories,
            destination: text(repo, reserved_keys::BACKUP_SCHEDULE_DESTINATION)
                .await?
                .map(PathBuf::from),
            key: recipient
                .zip(wrapped_identity)
                .map(|(recipient, wrapped_identity)| ScheduleKey {
                    recipient,
                    wrapped_identity,
                }),
        })
    }
}

async fn text(repo: &dyn SettingsRepo, key: &str) -> Result<Option<String>, StorageError> {
    Ok(repo
        .get_string(key)
        .await?
        .filter(|value| !value.trim().is_empty()))
}

async fn bounded(
    repo: &dyn SettingsRepo,
    key: &str,
    (min, default, max): (u32, u32, u32),
) -> Result<u32, StorageError> {
    Ok(text(repo, key)
        .await?
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .map_or(default, |value| value.clamp(min, max)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Progress {
    pub last_success_at: Option<OffsetDateTime>,
    pub fingerprint: Option<String>,
}

impl Progress {
    pub(crate) async fn load(repo: &dyn SettingsRepo) -> Result<Self, StorageError> {
        Ok(Self {
            last_success_at: text(repo, reserved_keys::BACKUP_SCHEDULE_LAST_SUCCESS_AT)
                .await?
                .and_then(|raw| OffsetDateTime::parse(&raw, &Rfc3339).ok()),
            fingerprint: text(repo, reserved_keys::BACKUP_SCHEDULE_CONTENT_FINGERPRINT).await?,
        })
    }

    pub(crate) async fn save(&self, repo: &dyn SettingsRepo) -> Result<(), StorageError> {
        if let Some(at) = self.last_success_at.and_then(|at| at.format(&Rfc3339).ok()) {
            repo.set_string(reserved_keys::BACKUP_SCHEDULE_LAST_SUCCESS_AT, &at)
                .await?;
        }
        if let Some(fingerprint) = &self.fingerprint {
            repo.set_string(
                reserved_keys::BACKUP_SCHEDULE_CONTENT_FINGERPRINT,
                fingerprint,
            )
            .await?;
        }
        Ok(())
    }
}
