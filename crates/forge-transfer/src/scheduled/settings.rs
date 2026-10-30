use std::path::PathBuf;

use forge_storage::settings::core_settings::{
    BACKUP_SCHEDULE_CONTENT_FINGERPRINT, BACKUP_SCHEDULE_DESTINATION, BACKUP_SCHEDULE_ENABLED,
    BACKUP_SCHEDULE_INCLUDE_ASSETS, BACKUP_SCHEDULE_INCLUDE_MEDIA,
    BACKUP_SCHEDULE_INCLUDE_OVERLAYS, BACKUP_SCHEDULE_INCLUDE_VOICES,
    BACKUP_SCHEDULE_INTERVAL_MINUTES, BACKUP_SCHEDULE_KEEP_DAILY_DAYS, BACKUP_SCHEDULE_KEEP_NEWEST,
    BACKUP_SCHEDULE_KEY, BACKUP_SCHEDULE_LAST_SUCCESS_AT,
};
use forge_storage::settings::{
    DEFAULT_BACKUP_SCHEDULE_ENABLED, DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES,
    DEFAULT_BACKUP_SCHEDULE_KEEP_DAILY_DAYS, DEFAULT_BACKUP_SCHEDULE_KEEP_NEWEST,
};
use forge_storage::{
    Flag, Setting, SettingsRepo, StorageError, read_setting, read_setting_or_default, write_setting,
};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

use crate::bundle::category::BackupCategory;
use crate::bundle::schedule_key::ScheduleKey;
use crate::bundle::settings_read::optional_text;
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
        let include = |setting: &'static Setting<Flag>, category: BackupCategory| async move {
            read_setting_or_default(repo, setting)
                .await
                .then_some(category)
        };
        let optional_categories = [
            include(&BACKUP_SCHEDULE_INCLUDE_MEDIA, BackupCategory::Media).await,
            include(&BACKUP_SCHEDULE_INCLUDE_VOICES, BackupCategory::Voices).await,
            include(&BACKUP_SCHEDULE_INCLUDE_OVERLAYS, BackupCategory::Overlays).await,
            include(&BACKUP_SCHEDULE_INCLUDE_ASSETS, BackupCategory::Assets).await,
        ];
        let categories = std::iter::once(BackupCategory::Database)
            .chain(optional_categories.into_iter().flatten())
            .collect();
        let stored_key = optional_text(repo, &BACKUP_SCHEDULE_KEY).await?;
        let decoded_key = stored_key.as_deref().map(ScheduleKey::decode);
        let key_unreadable = matches!(decoded_key, Some(Err(_)));
        Ok(Self {
            enabled: read_setting_or_default(repo, &BACKUP_SCHEDULE_ENABLED).await,
            interval_minutes: read_setting(repo, &BACKUP_SCHEDULE_INTERVAL_MINUTES).await?,
            retention: RetentionPolicy {
                keep_newest: read_setting(repo, &BACKUP_SCHEDULE_KEEP_NEWEST).await?,
                keep_daily_days: read_setting(repo, &BACKUP_SCHEDULE_KEEP_DAILY_DAYS).await?,
            },
            categories,
            destination: optional_text(repo, &BACKUP_SCHEDULE_DESTINATION)
                .await?
                .map(PathBuf::from),
            key: decoded_key.and_then(Result::ok),
            key_unreadable,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Progress {
    pub last_success_at: Option<OffsetDateTime>,
    pub fingerprint: Option<String>,
}

impl Progress {
    pub(crate) async fn load(repo: &dyn SettingsRepo) -> Result<Self, StorageError> {
        Ok(Self {
            last_success_at: optional_text(repo, &BACKUP_SCHEDULE_LAST_SUCCESS_AT)
                .await?
                .and_then(|raw| OffsetDateTime::parse(&raw, &Rfc3339).ok()),
            fingerprint: optional_text(repo, &BACKUP_SCHEDULE_CONTENT_FINGERPRINT).await?,
        })
    }

    pub(crate) async fn save(&self, repo: &dyn SettingsRepo) -> Result<(), StorageError> {
        if let Some(at) = self.last_success_at.and_then(|at| at.format(&Rfc3339).ok()) {
            write_setting(repo, &BACKUP_SCHEDULE_LAST_SUCCESS_AT, &Some(at)).await?;
        }
        if let Some(fingerprint) = &self.fingerprint {
            write_setting(
                repo,
                &BACKUP_SCHEDULE_CONTENT_FINGERPRINT,
                &Some(fingerprint.clone()),
            )
            .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::settings::core_settings::{
        BACKUP_SCHEDULE_INCLUDE_ASSETS, BACKUP_SCHEDULE_INCLUDE_MEDIA,
        BACKUP_SCHEDULE_INCLUDE_OVERLAYS, BACKUP_SCHEDULE_INCLUDE_VOICES,
        BACKUP_SCHEDULE_INTERVAL_MINUTES, BACKUP_SCHEDULE_KEY, BACKUP_SCHEDULE_LAST_SUCCESS_AT,
    };
    use forge_storage::settings::{
        DEFAULT_BACKUP_SCHEDULE_INTERVAL_MINUTES, MAX_BACKUP_SCHEDULE_INTERVAL_MINUTES,
        MIN_BACKUP_SCHEDULE_INTERVAL_MINUTES,
    };
    use forge_storage::test_support::MemorySettings;
    use time::{Date, Month};

    use super::{Progress, ScheduleSettings};
    use crate::bundle::category::BackupCategory;

    #[tokio::test]
    async fn interval_outside_the_bounds_is_clamped_and_garbage_falls_back_to_the_default() {
        let repo = MemorySettings::new();
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
            repo.seed_raw(&BACKUP_SCHEDULE_INTERVAL_MINUTES, &stored);

            let loaded = ScheduleSettings::load(&repo).await.unwrap();

            assert_eq!(loaded.interval_minutes, expected, "{stored:?}");
        }
    }

    #[tokio::test]
    async fn database_stays_included_when_every_optional_category_is_off() {
        let repo = MemorySettings::new();
        for setting in [
            &BACKUP_SCHEDULE_INCLUDE_MEDIA,
            &BACKUP_SCHEDULE_INCLUDE_VOICES,
            &BACKUP_SCHEDULE_INCLUDE_OVERLAYS,
            &BACKUP_SCHEDULE_INCLUDE_ASSETS,
        ] {
            repo.seed(setting, &false).unwrap();
        }

        let loaded = ScheduleSettings::load(&repo).await.unwrap();

        assert_eq!(loaded.categories, [BackupCategory::Database]);
    }

    #[tokio::test]
    async fn a_stored_key_loads_and_an_undecodable_one_is_flagged_unreadable() {
        let repo = MemorySettings::new();
        let key = crate::bundle::schedule_key::ScheduleKey {
            recipient: "age1recipient".to_owned(),
            wrapped_identity: "wrapped".to_owned(),
        };
        let mut loaded = Vec::new();
        for stored in [
            None,
            Some(key.encode().unwrap()),
            Some("{not json".to_owned()),
        ] {
            repo.seed(&BACKUP_SCHEDULE_KEY, &stored).unwrap();
            let settings = ScheduleSettings::load(&repo).await.unwrap();
            loaded.push((settings.key, settings.key_unreadable));
        }

        assert_eq!(
            loaded,
            vec![(None, false), (Some(key), false), (None, true)]
        );
    }

    #[tokio::test]
    async fn progress_round_trips_through_machine_local_settings() {
        let repo = MemorySettings::new();
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
    }

    #[tokio::test]
    async fn an_unparseable_last_success_time_reads_as_never_ran() {
        let repo = MemorySettings::new();
        repo.seed_raw(&BACKUP_SCHEDULE_LAST_SUCCESS_AT, "yesterday");

        let loaded = Progress::load(&repo).await.unwrap();

        assert_eq!(loaded.last_success_at, None);
    }
}
