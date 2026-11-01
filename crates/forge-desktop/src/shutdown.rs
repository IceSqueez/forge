use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{App, Global};
use tokio::runtime::Runtime;

const _: () =
    assert!(forge_host::stop::GRACEFUL_BUDGET.as_millis() < gpui::SHUTDOWN_TIMEOUT.as_millis());

pub(crate) const RUNTIME_SHUTDOWN_BUDGET: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub(crate) struct RuntimeSlot(Rc<RefCell<Option<Runtime>>>);

impl Global for RuntimeSlot {}

impl RuntimeSlot {
    pub(crate) fn new(runtime: Runtime) -> Self {
        Self(Rc::new(RefCell::new(Some(runtime))))
    }

    pub(crate) fn take(&self) -> Option<Runtime> {
        self.0.borrow_mut().take()
    }
}

pub(crate) fn silence_logging() {
    log::set_max_level(log::LevelFilter::Off);
}

pub(crate) struct QuitTeardown(Option<Runtime>);

impl QuitTeardown {
    pub(crate) fn claim(cx: &App) -> Self {
        if cfg!(target_os = "macos") {
            Self(cx.try_global::<RuntimeSlot>().and_then(RuntimeSlot::take))
        } else {
            Self(None)
        }
    }

    pub(crate) fn finish(self) {
        let Some(runtime) = self.0 else {
            return;
        };
        if std::thread::spawn(move || runtime.shutdown_timeout(RUNTIME_SHUTDOWN_BUDGET))
            .join()
            .is_err()
        {
            tracing::warn!("runtime shutdown thread panicked");
        }
        silence_logging();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn teardown_with_stuck_blocking_task() -> (QuitTeardown, std::sync::mpsc::Sender<()>) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let (release, parked) = std::sync::mpsc::channel::<()>();
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        runtime.spawn_blocking(move || {
            started_tx.send(()).unwrap();
            #[expect(
                clippy::let_underscore_must_use,
                reason = "recv returns once the sender is dropped"
            )]
            let _ = parked.recv();
        });
        started_rx.recv().unwrap();
        (QuitTeardown(Some(runtime)), release)
    }

    #[test]
    fn finish_abandons_a_stuck_blocking_task_and_silences_logging() {
        let (teardown, release) = teardown_with_stuck_blocking_task();
        log::set_max_level(log::LevelFilter::Info);
        let (finished, returned) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            teardown.finish();
            finished.send(()).unwrap();
        });

        returned
            .recv_timeout(forge_test_support::HANG_GUARD)
            .expect("finish waited on the stuck task");

        assert!(release.send(()).is_ok(), "the task was not stuck any more");
        assert_eq!(log::max_level(), log::LevelFilter::Off);
    }

    #[gpui::test]
    fn claim_takes_the_runtime_out_of_the_slot_only_on_macos_and_only_once(
        cx: &mut gpui::TestAppContext,
    ) {
        let slot = RuntimeSlot::new(
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap(),
        );
        cx.update(|cx| cx.set_global(slot.clone()));

        let (first, second) = cx.update(|cx| (QuitTeardown::claim(cx), QuitTeardown::claim(cx)));

        let macos = cfg!(target_os = "macos");
        assert_eq!(first.0.is_some(), macos);
        assert!(second.0.is_none());
        assert_eq!(slot.take().is_some(), !macos);
    }

    #[gpui::test]
    fn claim_without_an_installed_slot_yields_nothing(cx: &mut gpui::TestAppContext) {
        let claimed = cx.update(|cx| QuitTeardown::claim(cx));

        assert!(claimed.0.is_none());
    }
}
