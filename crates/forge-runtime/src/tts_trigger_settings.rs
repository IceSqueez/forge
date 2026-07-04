use std::sync::{Arc, PoisonError, RwLock};

use forge_storage::TtsTriggerSettings;

/// Live-swappable holder for the TTS source-gating toggles the speak runner
/// consults before each speak. The UI swaps a fresh snapshot in on save; the
/// runner reads the current snapshot under a short guard that never spans an
/// `.await`.
#[derive(Clone)]
pub struct TtsTriggerSettingsHandle(Arc<RwLock<TtsTriggerSettings>>);

impl TtsTriggerSettingsHandle {
    pub fn new(settings: TtsTriggerSettings) -> Self {
        Self(Arc::new(RwLock::new(settings)))
    }

    pub fn load(&self) -> TtsTriggerSettings {
        *self.0.read().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn swap(&self, settings: TtsTriggerSettings) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = settings;
    }
}
