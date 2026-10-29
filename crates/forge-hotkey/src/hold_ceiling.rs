use forge_storage::SettingDisclosure::Withheld;
use forge_storage::SettingPortability::Portable;
use forge_storage::{SettingEncoding, SettingSpec, SettingsRepo, StorageError};

use crate::config::DEFAULT_HOLD_CEILING_SECS;

pub const HOLD_CEILING: SettingSpec = SettingSpec::exact(
    "hotkey.hold_ceiling_secs",
    SettingEncoding::Integer,
    Portable,
    Withheld,
);

pub const HOTKEY_HOLD_CEILING_KEY: &str = HOLD_CEILING.key.name();

pub const SETTING_SPECS: &[SettingSpec] = &[HOLD_CEILING];
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

#[cfg(test)]
mod tests {
    use forge_storage::SettingsCatalog;

    use super::SETTING_SPECS;

    #[test]
    fn declared_settings_match_the_core_catalog_rows_they_duplicate() {
        let core = SettingsCatalog::core();
        for spec in SETTING_SPECS {
            let core_row = core
                .specs()
                .iter()
                .find(|row| row.key.name() == spec.key.name());

            assert_eq!(core_row, Some(spec), "{}", spec.key.name());
        }
    }
}
