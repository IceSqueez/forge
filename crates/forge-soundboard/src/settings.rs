use std::sync::Arc;

use forge_storage::SettingsRepo;
use forge_types::{OutputDevice, Shared};

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
            enabled: true,
            output_device_id: None,
            master_volume: 1.0,
            also_headphones: false,
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
pub struct SoundboardSettingsHandle(Shared<SoundboardSettings>);

impl SoundboardSettingsHandle {
    pub fn new(initial: SoundboardSettings) -> Self {
        Self(Shared::new(initial))
    }

    pub fn load(&self) -> Arc<SoundboardSettings> {
        self.0.load()
    }

    pub fn swap(&self, settings: SoundboardSettings) {
        self.0.store(settings);
    }
}

impl Default for SoundboardSettingsHandle {
    fn default() -> Self {
        Self::new(SoundboardSettings::default())
    }
}

pub async fn load_soundboard_settings(repo: &dyn SettingsRepo) -> SoundboardSettings {
    SoundboardSettings {
        enabled: forge_storage::soundboard_enabled(repo)
            .await
            .unwrap_or(true),
        output_device_id: forge_storage::soundboard_output_device(repo)
            .await
            .unwrap_or(None),
        master_volume: forge_storage::soundboard_master_volume(repo)
            .await
            .unwrap_or(1.0),
        also_headphones: forge_storage::soundboard_also_headphones(repo)
            .await
            .unwrap_or(false),
    }
}
