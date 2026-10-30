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
    use forge_storage::settings::core_settings::{
        SAFETY_COPIES_DELETE_AFTER_DAYS, SAFETY_COPIES_KEEP_NEWEST, SAFETY_COPIES_MODE,
    };
    use forge_storage::settings::{
        MAX_SAFETY_COPIES_DELETE_AFTER_DAYS, MAX_SAFETY_COPIES_KEEP_NEWEST,
        MIN_SAFETY_COPIES_DELETE_AFTER_DAYS, MIN_SAFETY_COPIES_KEEP_NEWEST,
    };
    use forge_storage::test_support::MemorySettings;

    use super::{SafetyCopyMode, SafetyCopySettings, SafetyRetention};

    const KEEP: u32 = 7;
    const DAYS: u32 = 45;

    #[tokio::test]
    async fn an_untouched_store_loads_the_default_policy() {
        let repo = MemorySettings::new();

        let loaded = SafetyCopySettings::load(&repo).await.unwrap();

        assert_eq!(loaded, SafetyCopySettings::default());
    }

    #[tokio::test]
    async fn switching_modes_keeps_both_numbers_through_every_save() {
        let repo = MemorySettings::new();
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
        let repo = MemorySettings::new();
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
            repo.seed_raw(&SAFETY_COPIES_KEEP_NEWEST, &keep);
            repo.seed_raw(&SAFETY_COPIES_DELETE_AFTER_DAYS, &days);

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
        let repo = MemorySettings::new();
        SafetyCopySettings {
            mode: SafetyCopyMode::KeepNewest,
            keep_newest: 0,
            delete_after_days: u32::MAX,
        }
        .save(&repo)
        .await
        .unwrap();

        let stored = (
            repo.row(SAFETY_COPIES_KEEP_NEWEST.name())
                .map(|row| row.value),
            repo.row(SAFETY_COPIES_DELETE_AFTER_DAYS.name())
                .map(|row| row.value),
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
        let repo = MemorySettings::new();
        repo.seed_raw(&SAFETY_COPIES_MODE, "keep_forever");
        repo.seed_raw(&SAFETY_COPIES_KEEP_NEWEST, &KEEP.to_string());
        repo.seed_raw(&SAFETY_COPIES_DELETE_AFTER_DAYS, &DAYS.to_string());

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

    #[tokio::test]
    async fn only_the_exact_stored_mode_names_select_a_mode() {
        for (stored, expected) in [
            ("keep_newest", SafetyCopyMode::KeepNewest),
            ("delete_after_days", SafetyCopyMode::DeleteAfterDays),
            ("never", SafetyCopyMode::Never),
            (" never ", SafetyCopyMode::KeepNewest),
            ("NEVER", SafetyCopyMode::KeepNewest),
            ("delete_after_days\n", SafetyCopyMode::KeepNewest),
            ("", SafetyCopyMode::KeepNewest),
        ] {
            let repo = MemorySettings::new();
            repo.seed_raw(&SAFETY_COPIES_MODE, stored);

            let loaded = SafetyCopySettings::load(&repo).await.unwrap();

            assert_eq!(loaded.mode, expected, "stored {stored:?}");
        }
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
