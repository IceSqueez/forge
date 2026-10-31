use std::sync::Arc;

use forge_storage::SettingDisclosure::Verbatim;
use forge_storage::SettingPortability::MachineLocal;
use forge_storage::{
    Flag, Setting, SettingsRepo, StorageError, read_stored_setting, write_setting,
};

use crate::integration_lifecycle::is_desired_on;
use crate::navigation::Screen;
use forge_runtime::LifecycleStates;

pub(crate) const FIRST_RUN_COMPLETED: Setting<Flag> = Setting::new(
    "integrations.first_run_completed",
    Flag { default: false },
    MachineLocal,
    Verbatim,
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FirstRun {
    Welcome,
    Completed,
}

impl FirstRun {
    pub(crate) fn decide(completed: bool, any_enabled: bool) -> (Self, bool) {
        match (completed, any_enabled) {
            (true, _) => (FirstRun::Completed, false),
            (false, true) => (FirstRun::Completed, true),
            (false, false) => (FirstRun::Welcome, false),
        }
    }

    pub(crate) fn opening_screen(self, requested: Screen) -> Screen {
        match (self, &requested) {
            (FirstRun::Welcome, Screen::Home) => Screen::Welcome,
            _ => requested,
        }
    }
}

pub(crate) fn any_enabled(states: &LifecycleStates) -> bool {
    states.values().any(is_desired_on)
}

pub(crate) async fn resolve_first_run(
    settings: &dyn SettingsRepo,
    states: &LifecycleStates,
) -> FirstRun {
    let completed = match read_stored_setting(settings, &FIRST_RUN_COMPLETED).await {
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

pub(crate) async fn record_completed(settings: &dyn SettingsRepo) -> Result<(), StorageError> {
    write_setting(settings, &FIRST_RUN_COMPLETED, &true).await
}

pub(crate) fn spawn_record_completed(
    settings: Arc<dyn SettingsRepo>,
    rt_handle: &tokio::runtime::Handle,
) {
    rt_handle.spawn(async move {
        if let Err(e) = record_completed(settings.as_ref()).await {
            tracing::warn!(error = %e, "could not record that first run was completed");
        }
    });
}

#[cfg(test)]
mod tests {
    use forge_storage::test_support::MemorySettings;
    use forge_types::IntegrationId;

    use super::*;
    use crate::test_support::{SettingWrite, runtime, test_backend};
    use forge_runtime::LifecycleState;

    fn states(state: LifecycleState) -> LifecycleStates {
        LifecycleStates::from([
            (IntegrationId::new("twitch"), LifecycleState::Disabled),
            (IntegrationId::new("obs"), state),
        ])
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
        let flag = (FIRST_RUN_COMPLETED.name().to_owned(), "true".to_owned());
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
                settings.seed_raw(&FIRST_RUN_COMPLETED, value);
            }

            let resolved = rt.block_on(resolve_first_run(settings.as_ref(), &states(state)));

            let written: Vec<SettingWrite> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
            assert_eq!((resolved, written), (expected, writes), "{case}");
        }
    }

    #[test]
    fn an_unreadable_first_run_flag_skips_the_welcome_without_writing() {
        let rt = runtime();
        let settings = MemorySettings::new();
        settings.fail_reads(true);

        let resolved = rt.block_on(resolve_first_run(
            &settings,
            &states(LifecycleState::Disabled),
        ));

        assert_eq!(
            (resolved, settings.write_log()),
            (FirstRun::Completed, Vec::new())
        );
    }
}
