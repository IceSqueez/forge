use forge_storage::settings::core_settings::{
    SAFETY_COPIES_DELETE_AFTER_DAYS, SAFETY_COPIES_KEEP_NEWEST, SAFETY_COPIES_MODE,
};
use forge_storage::settings::{
    DEFAULT_SAFETY_COPIES_DELETE_AFTER_DAYS, DEFAULT_SAFETY_COPIES_KEEP_NEWEST,
    MAX_SAFETY_COPIES_DELETE_AFTER_DAYS, MAX_SAFETY_COPIES_KEEP_NEWEST,
    MIN_SAFETY_COPIES_DELETE_AFTER_DAYS, MIN_SAFETY_COPIES_KEEP_NEWEST,
    SAFETY_COPIES_MODE_DELETE_AFTER_DAYS, SAFETY_COPIES_MODE_KEEP_NEWEST, SAFETY_COPIES_MODE_NEVER,
};
use forge_storage::{SettingsRepo, StorageError, read_setting, write_setting};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SafetyCopyMode {
    #[default]
    KeepNewest,
    DeleteAfterDays,
    Never,
}

impl SafetyCopyMode {
    fn as_stored(self) -> &'static str {
        match self {
            Self::KeepNewest => SAFETY_COPIES_MODE_KEEP_NEWEST,
            Self::DeleteAfterDays => SAFETY_COPIES_MODE_DELETE_AFTER_DAYS,
            Self::Never => SAFETY_COPIES_MODE_NEVER,
        }
    }

    fn from_stored(raw: &str) -> Option<Self> {
        match raw.trim() {
            SAFETY_COPIES_MODE_KEEP_NEWEST => Some(Self::KeepNewest),
            SAFETY_COPIES_MODE_DELETE_AFTER_DAYS => Some(Self::DeleteAfterDays),
            SAFETY_COPIES_MODE_NEVER => Some(Self::Never),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafetyCopySettings {
    pub mode: SafetyCopyMode,
    pub keep_newest: u32,
    pub delete_after_days: u32,
}

impl Default for SafetyCopySettings {
    fn default() -> Self {
        Self {
            mode: SafetyCopyMode::default(),
            keep_newest: DEFAULT_SAFETY_COPIES_KEEP_NEWEST,
            delete_after_days: DEFAULT_SAFETY_COPIES_DELETE_AFTER_DAYS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SafetyRetention {
    KeepNewest(u32),
    DeleteAfterDays(u32),
    Never,
}

impl SafetyCopySettings {
    pub async fn load(repo: &dyn SettingsRepo) -> Result<Self, StorageError> {
        Ok(Self {
            mode: SafetyCopyMode::from_stored(&read_setting(repo, &SAFETY_COPIES_MODE).await?)
                .unwrap_or_default(),
            keep_newest: read_setting(repo, &SAFETY_COPIES_KEEP_NEWEST).await?,
            delete_after_days: read_setting(repo, &SAFETY_COPIES_DELETE_AFTER_DAYS).await?,
        })
    }

    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            mode: self.mode,
            keep_newest: self
                .keep_newest
                .clamp(MIN_SAFETY_COPIES_KEEP_NEWEST, MAX_SAFETY_COPIES_KEEP_NEWEST),
            delete_after_days: self.delete_after_days.clamp(
                MIN_SAFETY_COPIES_DELETE_AFTER_DAYS,
                MAX_SAFETY_COPIES_DELETE_AFTER_DAYS,
            ),
        }
    }

    pub async fn save(&self, repo: &dyn SettingsRepo) -> Result<(), StorageError> {
        let settings = self.clamped();
        write_setting(
            repo,
            &SAFETY_COPIES_MODE,
            &settings.mode.as_stored().to_owned(),
        )
        .await?;
        write_setting(repo, &SAFETY_COPIES_KEEP_NEWEST, &settings.keep_newest).await?;
        write_setting(
            repo,
            &SAFETY_COPIES_DELETE_AFTER_DAYS,
            &settings.delete_after_days,
        )
        .await
    }

    pub(crate) fn retention(&self) -> SafetyRetention {
        let settings = self.clamped();
        match settings.mode {
            SafetyCopyMode::KeepNewest => SafetyRetention::KeepNewest(settings.keep_newest),
            SafetyCopyMode::DeleteAfterDays => {
                SafetyRetention::DeleteAfterDays(settings.delete_after_days)
            }
            SafetyCopyMode::Never => SafetyRetention::Never,
        }
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::settings::{
        MAX_SAFETY_COPIES_DELETE_AFTER_DAYS, MAX_SAFETY_COPIES_KEEP_NEWEST,
        MIN_SAFETY_COPIES_DELETE_AFTER_DAYS, MIN_SAFETY_COPIES_KEEP_NEWEST,
    };
    use forge_storage::{DATABASE_FILE_NAME, SettingsRepo, reserved_keys};
    use forge_storage_sqlite::SqliteBackend;
    use tempfile::TempDir;

    use super::{SafetyCopyMode, SafetyCopySettings, SafetyRetention};

    const KEEP: u32 = 7;
    const DAYS: u32 = 45;

    async fn backend() -> (TempDir, SqliteBackend) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join(DATABASE_FILE_NAME).display());
        let backend = SqliteBackend::open_for_test(&url, [7; 32], dir.path().join("media"), None)
            .await
            .unwrap();
        (dir, backend)
    }

    async fn store(repo: &SqliteBackend, pairs: &[(&str, &str)]) {
        for (key, value) in pairs {
            repo.set_string(key, value).await.unwrap();
        }
    }

    #[tokio::test]
    async fn an_untouched_store_loads_the_default_policy() {
        let (_dir, repo) = backend().await;

        let loaded = SafetyCopySettings::load(&repo).await.unwrap();

        assert_eq!(loaded, SafetyCopySettings::default());
    }

    #[tokio::test]
    async fn switching_modes_keeps_both_numbers_through_every_save() {
        let (_dir, repo) = backend().await;
        let mut seen = Vec::new();
        for mode in [
            SafetyCopyMode::DeleteAfterDays,
            SafetyCopyMode::Never,
            SafetyCopyMode::KeepNewest,
        ] {
            let settings = SafetyCopySettings {
                mode,
                keep_newest: KEEP,
                delete_after_days: DAYS,
            };
            settings.save(&repo).await.unwrap();
            seen.push(SafetyCopySettings::load(&repo).await.unwrap() == settings);
        }

        assert_eq!(seen, [true, true, true]);
    }

    #[tokio::test]
    async fn stored_numbers_outside_the_bounds_load_clamped_to_the_nearest_bound() {
        let (_dir, repo) = backend().await;
        for (keep, days, expected) in [
            (
                (MIN_SAFETY_COPIES_KEEP_NEWEST - 1).to_string(),
                (MIN_SAFETY_COPIES_DELETE_AFTER_DAYS - 1).to_string(),
                (
                    MIN_SAFETY_COPIES_KEEP_NEWEST,
                    MIN_SAFETY_COPIES_DELETE_AFTER_DAYS,
                ),
            ),
            (
                MIN_SAFETY_COPIES_KEEP_NEWEST.to_string(),
                MIN_SAFETY_COPIES_DELETE_AFTER_DAYS.to_string(),
                (
                    MIN_SAFETY_COPIES_KEEP_NEWEST,
                    MIN_SAFETY_COPIES_DELETE_AFTER_DAYS,
                ),
            ),
            (
                MAX_SAFETY_COPIES_KEEP_NEWEST.to_string(),
                MAX_SAFETY_COPIES_DELETE_AFTER_DAYS.to_string(),
                (
                    MAX_SAFETY_COPIES_KEEP_NEWEST,
                    MAX_SAFETY_COPIES_DELETE_AFTER_DAYS,
                ),
            ),
            (
                (MAX_SAFETY_COPIES_KEEP_NEWEST + 1).to_string(),
                (MAX_SAFETY_COPIES_DELETE_AFTER_DAYS + 1).to_string(),
                (
                    MAX_SAFETY_COPIES_KEEP_NEWEST,
                    MAX_SAFETY_COPIES_DELETE_AFTER_DAYS,
                ),
            ),
        ] {
            store(
                &repo,
                &[
                    (reserved_keys::SAFETY_COPIES_KEEP_NEWEST, &keep),
                    (reserved_keys::SAFETY_COPIES_DELETE_AFTER_DAYS, &days),
                ],
            )
            .await;

            let loaded = SafetyCopySettings::load(&repo).await.unwrap();

            assert_eq!(
                (loaded.keep_newest, loaded.delete_after_days),
                expected,
                "{keep}/{days}"
            );
        }
    }

    #[tokio::test]
    async fn saving_numbers_outside_the_bounds_stores_them_clamped() {
        let (_dir, repo) = backend().await;
        SafetyCopySettings {
            mode: SafetyCopyMode::KeepNewest,
            keep_newest: 0,
            delete_after_days: u32::MAX,
        }
        .save(&repo)
        .await
        .unwrap();

        let stored = (
            repo.get_string(reserved_keys::SAFETY_COPIES_KEEP_NEWEST)
                .await
                .unwrap(),
            repo.get_string(reserved_keys::SAFETY_COPIES_DELETE_AFTER_DAYS)
                .await
                .unwrap(),
        );

        assert_eq!(
            stored,
            (
                Some(MIN_SAFETY_COPIES_KEEP_NEWEST.to_string()),
                Some(MAX_SAFETY_COPIES_DELETE_AFTER_DAYS.to_string())
            )
        );
    }

    #[tokio::test]
    async fn an_unknown_mode_falls_back_to_keep_newest_and_keeps_the_stored_numbers() {
        let (_dir, repo) = backend().await;
        store(
            &repo,
            &[
                (reserved_keys::SAFETY_COPIES_MODE, "keep_forever"),
                (reserved_keys::SAFETY_COPIES_KEEP_NEWEST, &KEEP.to_string()),
                (
                    reserved_keys::SAFETY_COPIES_DELETE_AFTER_DAYS,
                    &DAYS.to_string(),
                ),
            ],
        )
        .await;

        let loaded = SafetyCopySettings::load(&repo).await.unwrap();

        assert_eq!(
            loaded,
            SafetyCopySettings {
                mode: SafetyCopyMode::KeepNewest,
                keep_newest: KEEP,
                delete_after_days: DAYS,
            }
        );
    }

    #[test]
    fn each_mode_retains_by_its_own_number_clamped_into_bounds() {
        for (mode, expected) in [
            (
                SafetyCopyMode::KeepNewest,
                SafetyRetention::KeepNewest(MIN_SAFETY_COPIES_KEEP_NEWEST),
            ),
            (
                SafetyCopyMode::DeleteAfterDays,
                SafetyRetention::DeleteAfterDays(MAX_SAFETY_COPIES_DELETE_AFTER_DAYS),
            ),
            (SafetyCopyMode::Never, SafetyRetention::Never),
        ] {
            let settings = SafetyCopySettings {
                mode,
                keep_newest: 0,
                delete_after_days: u32::MAX,
            };

            assert_eq!(settings.retention(), expected, "{mode:?}");
        }
    }
}
