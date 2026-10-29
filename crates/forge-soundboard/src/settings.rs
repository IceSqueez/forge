use std::sync::{Arc, PoisonError, RwLock};

use forge_storage::SettingDisclosure::{Presence, Verbatim};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::settings::{FULL_LEVEL, SILENT_LEVEL};
use forge_storage::{
    Flag, Level, Setting, SettingEncoding, SettingSpec, SettingsRepo, StorageError,
    read_setting_or_default, write_setting,
};
use forge_types::OutputDevice;

pub const ENABLED: Setting<Flag> = Setting::new(
    "soundboard.enabled",
    Flag { default: true },
    Portable,
    Verbatim,
);

pub const OUTPUT_DEVICE: SettingSpec = SettingSpec::exact(
    "soundboard.output_device",
    SettingEncoding::Text,
    MachineLocal,
    Presence,
);

pub const MASTER_VOLUME: Setting<Level> = Setting::new(
    "soundboard.master_volume",
    Level {
        default: FULL_LEVEL,
        min: SILENT_LEVEL,
        max: FULL_LEVEL,
    },
    Portable,
    Verbatim,
);

pub const ALSO_HEADPHONES: Setting<Flag> = Setting::new(
    "soundboard.also_headphones",
    Flag { default: false },
    Portable,
    Verbatim,
);

pub const SETTING_SPECS: &[SettingSpec] = &[
    ENABLED.spec(),
    OUTPUT_DEVICE,
    MASTER_VOLUME.spec(),
    ALSO_HEADPHONES.spec(),
];

#[derive(Debug, Clone, PartialEq)]
pub struct SoundboardSettings {
    pub enabled: bool,
    pub output_device_id: Option<String>,
    pub master_volume: f32,
    pub also_headphones: bool,
}

impl Default for SoundboardSettings {
    fn default() -> Self {
        Self {
            enabled: ENABLED.codec().default,
            output_device_id: None,
            master_volume: MASTER_VOLUME.codec().default,
            also_headphones: ALSO_HEADPHONES.codec().default,
        }
    }
}

impl SoundboardSettings {
    pub fn output_device(&self) -> OutputDevice {
        match &self.output_device_id {
            Some(id) => OutputDevice::ById { id: id.clone() },
            None => OutputDevice::Default,
        }
    }
}

#[derive(Clone)]
pub struct SoundboardSettingsHandle(Arc<RwLock<Arc<SoundboardSettings>>>);

impl SoundboardSettingsHandle {
    pub fn new(initial: SoundboardSettings) -> Self {
        Self(Arc::new(RwLock::new(Arc::new(initial))))
    }

    pub fn load(&self) -> Arc<SoundboardSettings> {
        Arc::clone(&self.0.read().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn swap(&self, settings: SoundboardSettings) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(settings);
    }

    pub fn update(&self, change: impl FnOnce(&mut SoundboardSettings)) -> Arc<SoundboardSettings> {
        let mut guard = self.0.write().unwrap_or_else(PoisonError::into_inner);
        let mut next = (**guard).clone();
        change(&mut next);
        let next = Arc::new(next);
        *guard = Arc::clone(&next);
        next
    }
}

impl Default for SoundboardSettingsHandle {
    fn default() -> Self {
        Self::new(SoundboardSettings::default())
    }
}

pub async fn load_soundboard_settings(repo: &dyn SettingsRepo) -> SoundboardSettings {
    let output_device_id = match repo.get_string(OUTPUT_DEVICE.key.name()).await {
        Ok(device) => device,
        Err(error) => {
            tracing::warn!(
                key = OUTPUT_DEVICE.key.name(),
                error = %error,
                "stored setting cannot be read; using its default"
            );
            None
        }
    };
    SoundboardSettings {
        enabled: read_setting_or_default(repo, &ENABLED).await,
        output_device_id,
        master_volume: read_setting_or_default(repo, &MASTER_VOLUME).await,
        also_headphones: read_setting_or_default(repo, &ALSO_HEADPHONES).await,
    }
}

pub async fn save_master_volume(repo: &dyn SettingsRepo, volume: f32) -> Result<(), StorageError> {
    write_setting(repo, &MASTER_VOLUME, &volume).await
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use forge_storage::SettingsCatalog;

    use super::*;

    struct UnreachableStore;

    #[async_trait]
    impl SettingsRepo for UnreachableStore {
        async fn get_string(&self, _: &str) -> Result<Option<String>, StorageError> {
            Err(StorageError::Connection {
                reason: "settings store unreachable".to_owned(),
            })
        }
        async fn set_string(&self, _: &str, _: &str) -> Result<(), StorageError> {
            unreachable!("loading never writes")
        }
        async fn delete(&self, _: &str) -> Result<bool, StorageError> {
            unreachable!("loading never deletes")
        }
        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            unreachable!("loading reads single keys")
        }
    }

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

    #[tokio::test]
    async fn an_unreadable_store_loads_the_default_settings() {
        let loaded = load_soundboard_settings(&UnreachableStore).await;

        assert_eq!(loaded, SoundboardSettings::default());
    }
}
