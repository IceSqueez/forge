use std::sync::Arc;

use forge_storage::{SettingsRepo, StorageError};

use crate::integration_lifecycle::is_desired_on;
use crate::integration_supervisor::LifecycleStates;
use crate::screen::Screen;

pub const FIRST_RUN_COMPLETED_KEY: &str = "integrations.first_run_completed";
const COMPLETED: &str = "true";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRun {
    Welcome,
    Completed,
}

impl FirstRun {
    pub fn decide(completed: bool, any_enabled: bool) -> (Self, bool) {
        match (completed, any_enabled) {
            (true, _) => (FirstRun::Completed, false),
            (false, true) => (FirstRun::Completed, true),
            (false, false) => (FirstRun::Welcome, false),
        }
    }

    pub fn opening_screen(self, requested: Screen) -> Screen {
        match (self, &requested) {
            (FirstRun::Welcome, Screen::Home) => Screen::Welcome,
            _ => requested,
        }
    }
}

pub fn any_enabled(states: &LifecycleStates) -> bool {
    states.values().any(is_desired_on)
}

pub async fn resolve_first_run(settings: &dyn SettingsRepo, states: &LifecycleStates) -> FirstRun {
    let completed = match settings.get_string(FIRST_RUN_COMPLETED_KEY).await {
        Ok(stored) => stored.is_some(),
        Err(e) => {
            tracing::warn!(error = %e, "could not read whether first run was completed");
            true
        }
    };
    let (first_run, record) = FirstRun::decide(completed, any_enabled(states));
    if record && let Err(e) = record_completed(settings).await {
        tracing::warn!(error = %e, "could not record that first run was completed");
    }
    first_run
}

pub async fn record_completed(settings: &dyn SettingsRepo) -> Result<(), StorageError> {
    settings
        .set_string(FIRST_RUN_COMPLETED_KEY, COMPLETED)
        .await
}

pub fn spawn_record_completed(settings: Arc<dyn SettingsRepo>, rt_handle: &tokio::runtime::Handle) {
    rt_handle.spawn(async move {
        if let Err(e) = record_completed(settings.as_ref()).await {
            tracing::warn!(error = %e, "could not record that first run was completed");
        }
    });
}
