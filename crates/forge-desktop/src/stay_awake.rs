use std::sync::Arc;

use forge_awake::{StatusWatch, StayAwake, StayAwakeConfig};
use forge_storage::SettingDisclosure::Verbatim;
use forge_storage::SettingPortability::MachineLocal;
use forge_storage::{Flag, Setting, SettingsRepo, read_setting_or_default, write_setting};
use gpui::Context;

use crate::async_bridge::{SnapshotWriter, WriteOutcome};

pub(crate) const ENABLED_BY_DEFAULT: bool = true;

pub const STAY_AWAKE: Setting<Flag> = Setting::new(
    "app.stay_awake",
    Flag {
        default: ENABLED_BY_DEFAULT,
    },
    MachineLocal,
    Verbatim,
);

const HOLD_REASON: &str = "forge keeps the system and display awake while it runs";

#[derive(Clone)]
pub struct StayAwakeSetting {
    service: StayAwake,
    settings: Arc<dyn SettingsRepo>,
    writer: Arc<SnapshotWriter>,
}

impl StayAwakeSetting {
    pub fn watch(&self) -> StatusWatch {
        self.service.watch()
    }

    pub fn release(&self) {
        self.service.release();
    }

    pub fn set_enabled<V: 'static>(
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
                    .map_err(|e| e.to_string())?;
                service.set_enabled(enabled).map_err(|e| e.to_string())
            },
            on_settled,
            cx,
        );
    }
}

pub async fn start_stay_awake(settings: Arc<dyn SettingsRepo>) -> StayAwakeSetting {
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
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use forge_awake::{AwakeError, StayAwake, StayAwakeConfig};
    use forge_storage::{SettingsRepo, StorageError, get_bool_setting, reserved_keys};
    use gpui::{AppContext, Entity, TestAppContext};
    use tokio::runtime::Runtime;
    use tokio::sync::Semaphore;

    use super::{ENABLED_BY_DEFAULT, HOLD_REASON, StayAwakeSetting};
    use crate::async_bridge::{SnapshotWriter, WriteOutcome};
    use crate::test_support::{SETTINGS_WRITE_FAILED, pump, runtime, test_backend, writes_to};

    #[derive(Default)]
    struct Settled(Vec<WriteOutcome>);

    struct GatedSettings {
        gate: Semaphore,
        stored: Mutex<Vec<String>>,
        in_flight: AtomicUsize,
        most_in_flight: AtomicUsize,
    }

    impl GatedSettings {
        fn closed() -> Self {
            Self {
                gate: Semaphore::new(0),
                stored: Mutex::new(Vec::new()),
                in_flight: AtomicUsize::new(0),
                most_in_flight: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl SettingsRepo for GatedSettings {
        async fn get_string(&self, _: &str) -> Result<Option<String>, StorageError> {
            Ok(self.stored.lock().unwrap().last().cloned())
        }

        async fn set_string(&self, _: &str, value: &str) -> Result<(), StorageError> {
            let running = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.most_in_flight.fetch_max(running, Ordering::SeqCst);
            self.gate.acquire().await.unwrap().forget();
            self.stored.lock().unwrap().push(value.to_owned());
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        }

        async fn delete(&self, _: &str) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
            Ok(HashMap::new())
        }
    }

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

        let next_start = rt.block_on(get_bool_setting(
            settings.as_ref(),
            reserved_keys::STAY_AWAKE,
            ENABLED_BY_DEFAULT,
        ));
        assert_eq!(
            (outcomes(&settled, cx), next_start),
            (vec![WriteOutcome::Written], false)
        );
    }

    #[gpui::test]
    fn a_failed_store_settles_failed_without_reaching_the_service(cx: &mut TestAppContext) {
        let rt = runtime();
        let (settings, _writes) = test_backend();
        settings.fail_writes(reserved_keys::STAY_AWAKE);
        let setting = setting(stopped_service(), settings as _, &rt);
        let settled = cx.new(|_| Settled::default());

        toggle(&setting, &settled, false, cx);
        pump(&rt);
        cx.run_until_parked();

        assert!(
            matches!(
                outcomes(&settled, cx).as_slice(),
                [WriteOutcome::Failed(message)] if message.contains(SETTINGS_WRITE_FAILED)
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
                writes_to(&mut writes, reserved_keys::STAY_AWAKE)
            ),
            (
                vec![WriteOutcome::Failed(AwakeError::Stopped.to_string())],
                vec!["false".to_owned()]
            )
        );
    }

    #[gpui::test]
    fn rapid_toggles_store_one_at_a_time_and_end_on_the_last_choice(cx: &mut TestAppContext) {
        let rt = runtime();
        let parked = runtime();
        let settings = Arc::new(GatedSettings::closed());
        let setting = setting(parked_service(&parked), Arc::clone(&settings) as _, &rt);
        let settled = cx.new(|_| Settled::default());

        toggle(&setting, &settled, false, cx);
        pump(&rt);
        for enabled in [true, false, true] {
            toggle(&setting, &settled, enabled, cx);
        }
        settings.gate.add_permits(4);
        pump(&rt);
        cx.run_until_parked();

        assert_eq!(
            (
                settings.stored.lock().unwrap().clone(),
                settings.most_in_flight.load(Ordering::SeqCst),
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
