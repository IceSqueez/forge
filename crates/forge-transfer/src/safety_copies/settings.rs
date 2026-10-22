use forge_storage::settings::{
    DEFAULT_SAFETY_COPIES_DELETE_AFTER_DAYS, DEFAULT_SAFETY_COPIES_KEEP_NEWEST,
    MAX_SAFETY_COPIES_DELETE_AFTER_DAYS, MAX_SAFETY_COPIES_KEEP_NEWEST,
    MIN_SAFETY_COPIES_DELETE_AFTER_DAYS, MIN_SAFETY_COPIES_KEEP_NEWEST,
};
use forge_storage::{SettingsRepo, StorageError, reserved_keys};

use crate::scheduled::settings::{bounded, text};

const MODE_KEEP_NEWEST: &str = "keep_newest";
const MODE_DELETE_AFTER_DAYS: &str = "delete_after_days";
const MODE_NEVER: &str = "never";

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
            Self::KeepNewest => MODE_KEEP_NEWEST,
            Self::DeleteAfterDays => MODE_DELETE_AFTER_DAYS,
            Self::Never => MODE_NEVER,
        }
    }

    fn from_stored(raw: &str) -> Option<Self> {
        match raw.trim() {
            MODE_KEEP_NEWEST => Some(Self::KeepNewest),
            MODE_DELETE_AFTER_DAYS => Some(Self::DeleteAfterDays),
            MODE_NEVER => Some(Self::Never),
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
        let mode = text(repo, reserved_keys::SAFETY_COPIES_MODE)
            .await?
            .and_then(|raw| SafetyCopyMode::from_stored(&raw))
            .unwrap_or_default();
        Ok(Self {
            mode,
            keep_newest: bounded(
                repo,
                reserved_keys::SAFETY_COPIES_KEEP_NEWEST,
                (
                    MIN_SAFETY_COPIES_KEEP_NEWEST,
                    DEFAULT_SAFETY_COPIES_KEEP_NEWEST,
                    MAX_SAFETY_COPIES_KEEP_NEWEST,
                ),
            )
            .await?,
            delete_after_days: bounded(
                repo,
                reserved_keys::SAFETY_COPIES_DELETE_AFTER_DAYS,
                (
                    MIN_SAFETY_COPIES_DELETE_AFTER_DAYS,
                    DEFAULT_SAFETY_COPIES_DELETE_AFTER_DAYS,
                    MAX_SAFETY_COPIES_DELETE_AFTER_DAYS,
                ),
            )
            .await?,
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
        repo.set_string(reserved_keys::SAFETY_COPIES_MODE, settings.mode.as_stored())
            .await?;
        repo.set_string(
            reserved_keys::SAFETY_COPIES_KEEP_NEWEST,
            &settings.keep_newest.to_string(),
        )
        .await?;
        repo.set_string(
            reserved_keys::SAFETY_COPIES_DELETE_AFTER_DAYS,
            &settings.delete_after_days.to_string(),
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
