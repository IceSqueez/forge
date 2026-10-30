use forge_storage::SettingDisclosure::Withheld;
use forge_storage::SettingPortability::Portable;
use forge_storage::{
    Setting, SettingCodec, SettingEncoding, SettingSpec, SettingsRepo, StorageError,
    read_setting_or_default, write_setting,
};

use crate::config::DEFAULT_HOLD_CEILING_SECS;

const CEILING_OFF: u64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldCeilingCodec;

impl SettingCodec for HoldCeilingCodec {
    type Value = Option<u64>;
    const ENCODING: SettingEncoding = SettingEncoding::Integer;

    fn default_value(&self) -> Option<u64> {
        Some(DEFAULT_HOLD_CEILING_SECS)
    }

    fn decode(&self, raw: &str) -> Option<Option<u64>> {
        let secs = raw.trim().parse::<u64>().ok()?;
        Some((secs != CEILING_OFF).then_some(secs))
    }

    fn encode(&self, value: &Option<u64>) -> Option<String> {
        Some(value.unwrap_or(CEILING_OFF).to_string())
    }
}

pub const HOLD_CEILING: Setting<HoldCeilingCodec> = Setting::new(
    "hotkey.hold_ceiling_secs",
    HoldCeilingCodec,
    Portable,
    Withheld,
);

pub const HOTKEY_HOLD_CEILING_KEY: &str = HOLD_CEILING.name();

pub const SETTING_SPECS: &[SettingSpec] = &[HOLD_CEILING.spec()];

pub async fn load_hold_ceiling(repo: &dyn SettingsRepo) -> Option<u64> {
    read_setting_or_default(repo, &HOLD_CEILING).await
}

pub async fn save_hold_ceiling(
    repo: &dyn SettingsRepo,
    secs: Option<u64>,
) -> Result<(), StorageError> {
    write_setting(repo, &HOLD_CEILING, &secs).await
}
