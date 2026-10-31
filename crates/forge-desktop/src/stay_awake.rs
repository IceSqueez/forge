use std::sync::Arc;

use forge_awake::{StatusWatch, StayAwake, StayAwakeConfig};
use forge_storage::SettingDisclosure::Verbatim;
use forge_storage::SettingPortability::MachineLocal;
use forge_storage::{Flag, Setting, SettingsRepo, read_setting_or_default, write_setting};
use gpui::Context;

use crate::async_bridge::{SnapshotWriter, WriteOutcome};
use crate::repo_failure::RepoFailure;

pub(crate) const ENABLED_BY_DEFAULT: bool = true;

pub(crate) const STAY_AWAKE: Setting<Flag> = Setting::new(
    "app.stay_awake",
    Flag {
        default: ENABLED_BY_DEFAULT,
    },
    MachineLocal,
    Verbatim,
);

const HOLD_REASON: &str = "forge keeps the system and display awake while it runs";

#[derive(Clone)]
pub(crate) struct StayAwakeSetting {
    service: StayAwake,
    settings: Arc<dyn SettingsRepo>,
    writer: Arc<SnapshotWriter>,
}

impl StayAwakeSetting {
    pub(crate) fn watch(&self) -> StatusWatch {
        self.service.watch()
    }

    pub(crate) fn release(&self) {
        self.service.release();
    }

    pub(crate) fn set_enabled<V: 'static>(
        &self,
        enabled: bool,
        on_settled: impl FnOnce(&mut V, WriteOutcome, &mut Context<V>) + 'static,
        cx: &mut Context<V>,
    ) {
        let settings = Arc::clone(&self.settings);
        let service = self.service.clone();
        self.writer.write(
            async move {
                write_setting(settings.as_ref(), &STAY_AWAKE, &enabled)
                    .await
                    .map_err(RepoFailure::from)?;
                service.set_enabled(enabled).map_err(RepoFailure::from)
            },
            on_settled,
            cx,
        );
    }
}

pub(crate) async fn start_stay_awake(settings: Arc<dyn SettingsRepo>) -> StayAwakeSetting {
    let enabled = read_setting_or_default(settings.as_ref(), &STAY_AWAKE).await;
    let service = StayAwake::spawn(StayAwakeConfig {
        enabled,
        reason: HOLD_REASON.to_owned(),
    });
    StayAwakeSetting {
        service,
        settings,
        writer: Arc::new(SnapshotWriter::new(&tokio::runtime::Handle::current())),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use forge_awake::{AwakeError, StayAwake, StayAwakeConfig};
    use forge_storage::test_support::{MemorySettings, SettingsWrite};
    use forge_storage::{SettingsRepo, read_setting};
    use gpui::{AppContext, Entity, TestAppContext};
    use tokio::runtime::Runtime;

    use super::{ENABLED_BY_DEFAULT, HOLD_REASON, STAY_AWAKE, StayAwakeSetting};
    use crate::async_bridge::{SnapshotWriter, WriteOutcome};
    use crate::test_support::{SETTINGS_WRITE_FAILED, pump, runtime, test_backend, writes_to};

    #[derive(Default)]
    struct Settled(Vec<WriteOutcome>);

    fn parked_service(parked: &Runtime) -> StayAwake {
        let _entered = parked.enter();
        StayAwake::spawn(StayAwakeConfig {
            enabled: ENABLED_BY_DEFAULT,
            reason: HOLD_REASON.to_owned(),
        })
    }

    fn stopped_service() -> StayAwake {
        parked_service(&runtime())
    }

    fn setting(
        service: StayAwake,
        settings: Arc<dyn SettingsRepo>,
        rt: &Runtime,
    ) -> StayAwakeSetting {
        StayAwakeSetting {
            service,
            settings,
            writer: Arc::new(SnapshotWriter::new(rt.handle())),
        }
    }

    fn toggle(
        setting: &StayAwakeSetting,
        settled: &Entity<Settled>,
        enabled: bool,
        cx: &mut TestAppContext,
    ) {
        settled.update(cx, |_, cx| {
            setting.set_enabled(
                enabled,
                |settled: &mut Settled, outcome, _| settled.0.push(outcome),
                cx,
            );
        });
    }

    fn outcomes(settled: &Entity<Settled>, cx: &mut TestAppContext) -> Vec<WriteOutcome> {
        settled.read_with(cx, |settled, _| settled.0.clone())
    }

    #[gpui::test]
    fn an_opt_out_settles_written_and_is_what_the_next_start_reads(cx: &mut TestAppContext) {
        let rt = runtime();
        let parked = runtime();
        let (settings, _writes) = test_backend();
        let setting = setting(parked_service(&parked), Arc::clone(&settings) as _, &rt);
        let settled = cx.new(|_| Settled::default());

        toggle(&setting, &settled, false, cx);
        pump(&rt);
        cx.run_until_parked();

        let next_start = rt
            .block_on(read_setting(settings.as_ref(), &STAY_AWAKE))
            .unwrap();
        assert_eq!(
            (outcomes(&settled, cx), next_start),
            (vec![WriteOutcome::Written], false)
        );
    }

    #[gpui::test]
    fn a_failed_store_settles_failed_without_reaching_the_service(cx: &mut TestAppContext) {
        let rt = runtime();
        let (settings, _writes) = test_backend();
        settings.fail_writes(STAY_AWAKE.name());
        let setting = setting(stopped_service(), settings as _, &rt);
        let settled = cx.new(|_| Settled::default());

        toggle(&setting, &settled, false, cx);
        pump(&rt);
        cx.run_until_parked();

        assert!(
            matches!(
                outcomes(&settled, cx).as_slice(),
                [WriteOutcome::Failed(message)] if message.to_string().contains(SETTINGS_WRITE_FAILED)
            ),
            "{:?}",
            outcomes(&settled, cx)
        );
    }

    #[gpui::test]
    fn a_stopped_service_settles_failed_after_the_choice_was_stored(cx: &mut TestAppContext) {
        let rt = runtime();
        let (settings, mut writes) = test_backend();
        let setting = setting(stopped_service(), settings as _, &rt);
        let settled = cx.new(|_| Settled::default());

        toggle(&setting, &settled, false, cx);
        pump(&rt);
        cx.run_until_parked();

        assert_eq!(
            (
                outcomes(&settled, cx),
                writes_to(&mut writes, STAY_AWAKE.name())
            ),
            (
                vec![WriteOutcome::Failed(AwakeError::Stopped.into())],
                vec!["false".to_owned()]
            )
        );
    }

    #[gpui::test]
    fn rapid_toggles_store_one_at_a_time_and_end_on_the_last_choice(cx: &mut TestAppContext) {
        let rt = runtime();
        let parked = runtime();
        let settings = MemorySettings::new();
        settings.hold_writes();
        let setting = setting(parked_service(&parked), Arc::new(settings.clone()), &rt);
        let settled = cx.new(|_| Settled::default());

        toggle(&setting, &settled, false, cx);
        pump(&rt);
        for enabled in [true, false, true] {
            toggle(&setting, &settled, enabled, cx);
        }
        settings.release_writes();
        pump(&rt);
        cx.run_until_parked();

        let stored: Vec<String> = settings
            .write_log()
            .into_iter()
            .filter_map(|write| match write {
                SettingsWrite::Put { value, .. } => Some(value),
                SettingsWrite::Delete { .. } => None,
            })
            .collect();
        assert_eq!(
            (
                stored,
                settings.peak_concurrent_writes(),
                outcomes(&settled, cx).last().cloned()
            ),
            (
                vec!["false".to_owned(), "true".to_owned()],
                1,
                Some(WriteOutcome::Written)
            )
        );
    }
}
