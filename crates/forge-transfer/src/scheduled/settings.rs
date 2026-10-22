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
    pub key_unreadable: bool,
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
            key_unreadable: false,
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
        let stored_key = text(repo, reserved_keys::BACKUP_SCHEDULE_KEY).await?;
        let decoded_key = stored_key.as_deref().map(ScheduleKey::decode);
        let key_unreadable = matches!(decoded_key, Some(Err(_)));
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
            key: decoded_key.and_then(Result::ok),
            key_unreadable,
        })
    }
}

pub(crate) async fn text(
    repo: &dyn SettingsRepo,
    key: &str,
) -> Result<Option<String>, StorageError> {
    Ok(repo
        .get_string(key)
        .await?
        .filter(|value| !value.trim().is_empty()))
}

pub(crate) async fn bounded(
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_storage::settings::{
        DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES, MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES,
        MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES,
    };
    use forge_storage::{DATABASE_FILE_NAME, DataProvider, SettingsRepo, reserved_keys};
    use forge_storage_sqlite::SqliteBackend;
    use tempfile::TempDir;
    use time::{Date, Month};

    use super::{Progress, ScheduleSettings};
    use crate::category::BackupCategory;

    async fn backend() -> (TempDir, SqliteBackend) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend = SqliteBackend::open_for_test(&url, [7; 32], dir.path().join("media"), None)
            .await
            .unwrap();
        (dir, backend)
    }

    #[tokio::test]
    async fn interval_outside_the_bounds_is_clamped_and_garbage_falls_back_to_the_default() {
        let (_dir, repo) = backend().await;
        for (stored, expected) in [
            (
                (MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES - 1).to_string(),
                MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES,
            ),
            (
                MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES.to_string(),
                MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES,
            ),
            (
                (MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES + 1).to_string(),
                MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES,
            ),
            ("-5".to_owned(), DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES),
            (
                "hourly".to_owned(),
                DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES,
            ),
            (" 90 ".to_owned(), 90),
        ] {
            repo.set_string(reserved_keys::BACKUP_SCHEDULE_INTERVAL_MINUTES, &stored)
                .await
                .unwrap();

            let loaded = ScheduleSettings::load(&repo).await.unwrap();

            assert_eq!(loaded.interval_minutes, expected, "{stored:?}");
        }
        repo.shutdown().await;
    }

    #[tokio::test]
    async fn database_stays_included_when_every_optional_category_is_off() {
        let (_dir, repo) = backend().await;
        for key in [
            reserved_keys::BACKUP_SCHEDULE_INCLUDE_MEDIA,
            reserved_keys::BACKUP_SCHEDULE_INCLUDE_VOICES,
            reserved_keys::BACKUP_SCHEDULE_INCLUDE_OVERLAYS,
            reserved_keys::BACKUP_SCHEDULE_INCLUDE_ASSETS,
        ] {
            repo.set_string(key, "false").await.unwrap();
        }

        let loaded = ScheduleSettings::load(&repo).await.unwrap();

        assert_eq!(loaded.categories, [BackupCategory::Database]);
        repo.shutdown().await;
    }

    #[tokio::test]
    async fn a_stored_key_loads_and_an_undecodable_one_is_flagged_unreadable() {
        let (_dir, repo) = backend().await;
        let key = crate::schedule_key::ScheduleKey {
            recipient: "age1recipient".to_owned(),
            wrapped_identity: "wrapped".to_owned(),
        };
        let mut loaded = Vec::new();
        for stored in [
            None,
            Some(key.encode().unwrap()),
            Some("{not json".to_owned()),
        ] {
            match &stored {
                Some(value) => repo
                    .set_string(reserved_keys::BACKUP_SCHEDULE_KEY, value)
                    .await
                    .unwrap(),
                None => {
                    repo.delete(reserved_keys::BACKUP_SCHEDULE_KEY)
                        .await
                        .unwrap();
                }
            }
            let settings = ScheduleSettings::load(&repo).await.unwrap();
            loaded.push((settings.key, settings.key_unreadable));
        }

        assert_eq!(
            loaded,
            vec![(None, false), (Some(key), false), (None, true)]
        );
        repo.shutdown().await;
    }

    #[tokio::test]
    async fn progress_round_trips_through_machine_local_settings() {
        let (_dir, repo) = backend().await;
        let progress = Progress {
            last_success_at: Some(
                Date::from_calendar_date(2026, Month::October, 21)
                    .unwrap()
                    .with_hms(12, 30, 5)
                    .unwrap()
                    .assume_utc(),
            ),
            fingerprint: Some("abc123".to_owned()),
        };

        progress.save(&repo).await.unwrap();

        assert_eq!(Progress::load(&repo).await.unwrap(), progress);
        repo.shutdown().await;
    }

    #[tokio::test]
    async fn an_unparseable_last_success_time_reads_as_never_ran() {
        let (_dir, repo) = backend().await;
        repo.set_string(reserved_keys::BACKUP_SCHEDULE_LAST_SUCCESS_AT, "yesterday")
            .await
            .unwrap();

        let loaded = Progress::load(&repo).await.unwrap();

        assert_eq!(loaded.last_success_at, None);
        repo.shutdown().await;
    }
}
