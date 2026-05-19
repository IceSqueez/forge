use std::collections::HashMap;

use async_trait::async_trait;

use crate::StorageError;

pub mod reserved_keys {
    pub const ONBOARDING_COMPLETED: &str = "onboarding_completed";
    pub const LAST_ONBOARDING_STEP: &str = "last_onboarding_step";
    pub const THEME: &str = "theme";
    pub const ACCENT_COLOR: &str = "accent_color";
    pub const DENSITY: &str = "density";
    pub const FONT_BODY: &str = "font_body";
    pub const FONT_MONO: &str = "font_mono";
    pub const EVENT_LOG_RETENTION_DAYS_KEY: &str = "event_log_retention_days";
}

#[async_trait]
pub trait SettingsRepo: Send + Sync {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError>;
    /// Returns `true` if the key was present and has been removed.
    async fn delete(&self, key: &str) -> Result<bool, StorageError>;
    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError>;

    async fn event_log_retention_days(&self) -> Result<u32, StorageError> {
        let raw = self
            .get_string(reserved_keys::EVENT_LOG_RETENTION_DAYS_KEY)
            .await?;
        Ok(raw.as_deref().and_then(|s| s.parse().ok()).unwrap_or(7))
    }

    async fn set_event_log_retention_days(&self, days: u32) -> Result<(), StorageError> {
        self.set_string(
            reserved_keys::EVENT_LOG_RETENTION_DAYS_KEY,
            &days.to_string(),
        )
        .await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::reserved_keys::*;

    fn _trait_is_dyn_safe(_: &dyn super::SettingsRepo) {}

    #[test]
    fn reserved_keys_are_non_empty() {
        assert!(!ONBOARDING_COMPLETED.is_empty());
        assert!(!LAST_ONBOARDING_STEP.is_empty());
        assert!(!THEME.is_empty());
        assert!(!ACCENT_COLOR.is_empty());
        assert!(!DENSITY.is_empty());
        assert!(!FONT_BODY.is_empty());
        assert!(!FONT_MONO.is_empty());
        assert!(!EVENT_LOG_RETENTION_DAYS_KEY.is_empty());
    }

    #[test]
    fn reserved_keys_are_distinct() {
        let keys = [
            ONBOARDING_COMPLETED,
            LAST_ONBOARDING_STEP,
            THEME,
            ACCENT_COLOR,
            DENSITY,
            FONT_BODY,
            FONT_MONO,
            EVENT_LOG_RETENTION_DAYS_KEY,
        ];
        let unique: std::collections::HashSet<&str> = keys.iter().copied().collect();
        assert_eq!(unique.len(), keys.len());
    }
}
