use forge_storage::{SettingsRepo, StorageError};

use crate::config::DEFAULT_HOLD_CEILING_SECS;

pub const HOTKEY_HOLD_CEILING_KEY: &str = "hotkey.hold_ceiling_secs";
const CEILING_OFF: u64 = 0;

pub async fn load_hold_ceiling(repo: &dyn SettingsRepo) -> Option<u64> {
    match repo.get_string(HOTKEY_HOLD_CEILING_KEY).await {
        Ok(Some(raw)) => match raw.trim().parse::<u64>() {
            Ok(CEILING_OFF) => None,
            Ok(secs) => Some(secs),
            Err(_) => Some(DEFAULT_HOLD_CEILING_SECS),
        },
        Ok(None) => Some(DEFAULT_HOLD_CEILING_SECS),
        Err(e) => {
            tracing::warn!(error = %e, "failed to read the hotkey hold ceiling");
            Some(DEFAULT_HOLD_CEILING_SECS)
        }
    }
}

pub async fn save_hold_ceiling(
    repo: &dyn SettingsRepo,
    secs: Option<u64>,
) -> Result<(), StorageError> {
    repo.set_string(
        HOTKEY_HOLD_CEILING_KEY,
        &secs.unwrap_or(CEILING_OFF).to_string(),
    )
    .await
}
