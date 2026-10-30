use std::sync::{Arc, PoisonError, RwLock};

use forge_storage::SettingDisclosure::{Presence, Verbatim};
use forge_storage::SettingPortability::{MachineLocal, Portable};
use forge_storage::settings::{FULL_LEVEL, SILENT_LEVEL};
use forge_storage::{
    Flag, Level, OptionalText, Setting, SettingSpec, SettingsRepo, StorageError, read_setting,
    write_setting,
};
use forge_types::OutputDevice;
use tokio::sync::watch;

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
pub struct SoundboardSettingsHandle {
    current: Arc<RwLock<Arc<SoundboardSettings>>>,
    revision: Arc<watch::Sender<u64>>,
}

pub struct SoundboardSettingsChanges(watch::Receiver<u64>);

impl SoundboardSettingsChanges {
    pub async fn changed(&mut self) -> bool {
        self.0.changed().await.is_ok()
    }
}

impl SoundboardSettingsHandle {
    pub fn new(initial: SoundboardSettings) -> Self {
        let (revision, _) = watch::channel(0);
        Self {
            current: Arc::new(RwLock::new(Arc::new(initial))),
            revision: Arc::new(revision),
        }
    }

    pub fn load(&self) -> Arc<SoundboardSettings> {
        Arc::clone(&self.current.read().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn changes(&self) -> SoundboardSettingsChanges {
        SoundboardSettingsChanges(self.revision.subscribe())
    }

    pub fn swap(&self, settings: SoundboardSettings) {
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(settings);
        self.announce_change();
    }

    pub fn update(&self, change: impl FnOnce(&mut SoundboardSettings)) -> Arc<SoundboardSettings> {
        let next = {
            let mut guard = self.current.write().unwrap_or_else(PoisonError::into_inner);
            let mut next = (**guard).clone();
            change(&mut next);
            let next = Arc::new(next);
            *guard = Arc::clone(&next);
            next
        };
        self.announce_change();
        next
    }

    fn announce_change(&self) {
        self.revision
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }
}

impl Default for SoundboardSettingsHandle {
    fn default() -> Self {
        Self::new(SoundboardSettings::default())
    }
}

async fn read_soundboard_settings(
    repo: &dyn SettingsRepo,
) -> Result<SoundboardSettings, StorageError> {
    Ok(SoundboardSettings {
        enabled: read_setting(repo, &ENABLED).await?,
        output_device_id: read_setting(repo, &OUTPUT_DEVICE).await?,
        master_volume: read_setting(repo, &MASTER_VOLUME).await?,
        also_headphones: read_setting(repo, &ALSO_HEADPHONES).await?,
    })
}

pub async fn load_soundboard_settings(repo: &dyn SettingsRepo) -> SoundboardSettings {
    read_soundboard_settings(repo)
        .await
        .unwrap_or_else(|error| {
            tracing::error!(%error, "soundboard settings cannot be read; the soundboard stays off");
            SoundboardSettings {
                enabled: false,
                ..SoundboardSettings::default()
            }
        })
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
    use std::collections::BTreeMap;

    use forge_storage::test_support::MemorySettings;

    use super::*;

    fn golden_rows(store: &MemorySettings) -> BTreeMap<String, String> {
        store
            .rows()
            .into_iter()
            .map(|row| (row.key, row.value))
            .collect()
    }

    #[tokio::test]
    async fn a_listener_is_woken_by_swap_and_update_and_reads_the_new_settings() {
        let changes: [fn(&SoundboardSettingsHandle); 2] = [
            |handle| {
                handle.swap(SoundboardSettings {
                    master_volume: 0.25,
                    ..SoundboardSettings::default()
                });
            },
            |handle| {
                handle.update(|settings| settings.master_volume = 0.25);
            },
        ];
        for change in changes {
            let handle = SoundboardSettingsHandle::default();
            let mut listener = handle.changes();

            change(&handle);

            assert!(listener.changed().await);
            assert_eq!(handle.load().master_volume, 0.25);
        }
    }

    #[tokio::test]
    async fn changes_end_once_every_settings_handle_is_gone() {
        let handle = SoundboardSettingsHandle::default();
        let mut listener = handle.changes();

        drop(handle);

        assert!(!listener.changed().await);
    }

    #[tokio::test]
    async fn saved_soundboard_settings_store_their_golden_strings_and_load_back() {
        let store = MemorySettings::new();
        save_enabled(&store, false).await.unwrap();
        save_output_device(&store, Some("hw:2,0".to_owned()))
            .await
            .unwrap();
        save_master_volume(&store, 0.75).await.unwrap();
        save_also_headphones(&store, true).await.unwrap();

        let loaded = load_soundboard_settings(&store).await;

        assert_eq!(
            (golden_rows(&store), loaded),
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
        let store = MemorySettings::new();
        save_output_device(&store, Some("hw:2,0".to_owned()))
            .await
            .unwrap();

        save_output_device(&store, None).await.unwrap();

        assert_eq!(
            (
                golden_rows(&store),
                load_soundboard_settings(&store).await.output_device()
            ),
            (BTreeMap::new(), OutputDevice::Default)
        );
    }

    #[tokio::test]
    async fn an_unreadable_store_loads_the_default_settings_with_the_soundboard_off() {
        let store = MemorySettings::new();
        store.seed(&ENABLED, &true).unwrap();
        store.fail_reads(true);

        let loaded = load_soundboard_settings(&store).await;

        assert_eq!(
            loaded,
            SoundboardSettings {
                enabled: false,
                ..SoundboardSettings::default()
            }
        );
    }

    #[tokio::test]
    async fn one_unreadable_key_keeps_the_soundboard_off_though_its_switch_is_stored_on() {
        for key in [
            ENABLED.name(),
            OUTPUT_DEVICE.name(),
            MASTER_VOLUME.name(),
            ALSO_HEADPHONES.name(),
        ] {
            let store = MemorySettings::new();
            store.seed(&ENABLED, &true).unwrap();
            store.fail_reads_for(key);

            let loaded = load_soundboard_settings(&store).await;

            assert!(!loaded.enabled, "{key}");
        }
    }
}
