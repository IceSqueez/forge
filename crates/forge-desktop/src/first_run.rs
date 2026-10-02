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

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use forge_types::IntegrationId;

    use super::*;
    use crate::integration_supervisor::LifecycleState;
    use crate::test_support::{SettingWrite, runtime, test_backend};

    fn states(state: LifecycleState) -> LifecycleStates {
        LifecycleStates::from([
            (IntegrationId::new("twitch"), LifecycleState::Disabled),
            (IntegrationId::new("obs"), state),
        ])
    }

    struct UnreadableSettings;

    #[async_trait]
    impl SettingsRepo for UnreadableSettings {
        async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
            Err(StorageError::NotFound {
                key: key.to_owned(),
            })
        }

        async fn set_string(&self, key: &str, _: &str) -> Result<(), StorageError> {
            panic!("an unreadable flag must not be written, got {key}");
        }

        async fn delete(&self, _: &str) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(HashMap::new())
        }
    }

    #[test]
    fn the_welcome_shows_only_before_first_run_and_with_everything_off() {
        for (completed, any_enabled, expected, case) in [
            (false, false, (FirstRun::Welcome, false), "fresh install"),
            (
                false,
                true,
                (FirstRun::Completed, true),
                "upgrade with integrations on",
            ),
            (
                true,
                false,
                (FirstRun::Completed, false),
                "user switched all off later",
            ),
            (true, true, (FirstRun::Completed, false), "returning user"),
        ] {
            assert_eq!(FirstRun::decide(completed, any_enabled), expected, "{case}");
        }
    }

    #[test]
    fn the_welcome_replaces_only_the_home_screen() {
        let detail = Screen::BuiltinDetail(IntegrationId::new("obs"));
        for (first_run, requested, expected) in [
            (FirstRun::Welcome, Screen::Home, Screen::Welcome),
            (FirstRun::Welcome, Screen::Chat, Screen::Chat),
            (FirstRun::Welcome, detail.clone(), detail),
            (FirstRun::Completed, Screen::Home, Screen::Home),
        ] {
            assert_eq!(
                first_run.opening_screen(requested.clone()),
                expected,
                "{first_run:?} opening {requested:?}"
            );
        }
    }

    #[test]
    fn resolving_records_completion_only_for_an_install_that_already_uses_integrations() {
        let rt = runtime();
        let flag = (FIRST_RUN_COMPLETED_KEY.to_owned(), "true".to_owned());
        for (stored, state, expected, writes, case) in [
            (
                None,
                LifecycleState::Running,
                FirstRun::Completed,
                vec![flag.clone()],
                "integrations on, no flag yet",
            ),
            (
                Some("true"),
                LifecycleState::Disabled,
                FirstRun::Completed,
                Vec::new(),
                "flag already stored",
            ),
            (
                None,
                LifecycleState::Disabled,
                FirstRun::Welcome,
                Vec::new(),
                "fresh install",
            ),
        ] {
            let (settings, mut rx) = test_backend();
            if let Some(value) = stored {
                rt.block_on(settings.set_string(FIRST_RUN_COMPLETED_KEY, value))
                    .unwrap();
                rx.try_recv().unwrap();
            }

            let resolved = rt.block_on(resolve_first_run(settings.as_ref(), &states(state)));

            let written: Vec<SettingWrite> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
            assert_eq!((resolved, written), (expected, writes), "{case}");
        }
    }

    #[test]
    fn an_unreadable_first_run_flag_skips_the_welcome_without_writing() {
        let rt = runtime();

        let resolved = rt.block_on(resolve_first_run(
            &UnreadableSettings,
            &states(LifecycleState::Disabled),
        ));

        assert_eq!(resolved, FirstRun::Completed);
    }
}
