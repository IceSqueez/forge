use std::sync::{Arc, PoisonError, RwLock};

use forge_storage::SettingDisclosure::{Presence, Verbatim};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::settings::{FULL_LEVEL, SILENT_LEVEL};
use forge_storage::{
    Flag, Level, OptionalText, Setting, SettingSpec, SettingsRepo, StorageError,
    read_setting_or_default, write_setting,
};
use forge_types::OutputDevice;

pub const ENABLED: Setting<Flag> = Setting::new(
    "soundboard.enabled",
    Flag { default: true },
    Portable,
    Verbatim,
);

pub const OUTPUT_DEVICE: Setting<OptionalText> = Setting::new(
    "soundboard.output_device",
    OptionalText,
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
    OUTPUT_DEVICE.spec(),
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
    SoundboardSettings {
        enabled: read_setting_or_default(repo, &ENABLED).await,
        output_device_id: read_setting_or_default(repo, &OUTPUT_DEVICE).await,
        master_volume: read_setting_or_default(repo, &MASTER_VOLUME).await,
        also_headphones: read_setting_or_default(repo, &ALSO_HEADPHONES).await,
    }
}

pub async fn save_master_volume(repo: &dyn SettingsRepo, volume: f32) -> Result<(), StorageError> {
    write_setting(repo, &MASTER_VOLUME, &volume).await
}

pub async fn save_enabled(repo: &dyn SettingsRepo, enabled: bool) -> Result<(), StorageError> {
    write_setting(repo, &ENABLED, &enabled).await
}

pub async fn save_also_headphones(
    repo: &dyn SettingsRepo,
    enabled: bool,
) -> Result<(), StorageError> {
    write_setting(repo, &ALSO_HEADPHONES, &enabled).await
}

pub async fn save_output_device(
    repo: &dyn SettingsRepo,
    device_id: Option<String>,
) -> Result<(), StorageError> {
    write_setting(repo, &OUTPUT_DEVICE, &device_id).await
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;

    #[derive(Default)]
    struct MapStore(Mutex<HashMap<String, String>>);

    impl MapStore {
        fn rows(&self) -> BTreeMap<String, String> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
                .into_iter()
                .collect()
        }
    }

    #[async_trait]
    impl SettingsRepo for MapStore {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(key)
                .cloned())
        }
        async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        async fn delete(&self, key: &str) -> Result<bool, StorageError> {
            Ok(self
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(key)
                .is_some())
        }
        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(self
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone())
        }
    }

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

    #[tokio::test]
    async fn saved_soundboard_settings_store_their_golden_strings_and_load_back() {
        let store = MapStore::default();
        save_enabled(&store, false).await.unwrap();
        save_output_device(&store, Some("hw:2,0".to_owned()))
            .await
            .unwrap();
        save_master_volume(&store, 0.75).await.unwrap();
        save_also_headphones(&store, true).await.unwrap();

        let loaded = load_soundboard_settings(&store).await;

        assert_eq!(
            (store.rows(), loaded),
            (
                BTreeMap::from(
                    [
                        ("soundboard.also_headphones", "true"),
                        ("soundboard.enabled", "false"),
                        ("soundboard.master_volume", "0.75"),
                        ("soundboard.output_device", "hw:2,0"),
                    ]
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
                ),
                SoundboardSettings {
                    enabled: false,
                    output_device_id: Some("hw:2,0".to_owned()),
                    master_volume: 0.75,
                    also_headphones: true,
                },
            )
        );
    }

    #[tokio::test]
    async fn clearing_the_output_device_deletes_its_row_and_loads_the_default_device() {
        let store = MapStore::default();
        save_output_device(&store, Some("hw:2,0".to_owned()))
            .await
            .unwrap();

        save_output_device(&store, None).await.unwrap();

        assert_eq!(
            (
                store.rows(),
                load_soundboard_settings(&store).await.output_device()
            ),
            (BTreeMap::new(), OutputDevice::Default)
        );
    }

    #[tokio::test]
    async fn an_unreadable_store_loads_the_default_settings() {
        let loaded = load_soundboard_settings(&UnreachableStore).await;

        assert_eq!(loaded, SoundboardSettings::default());
    }
}
