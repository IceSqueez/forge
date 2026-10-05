use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use forge_runtime::{
    ActionEngineHandle, EventBus, QueueSchedulerHandle, ScheduledRunsHandle, TimerSchedulerHandle,
    TriggerEvaluatorHandle,
};
use forge_server::ServerHandle;
use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};
use forge_storage::DataProvider;
use gpui::{App, Global};
use tokio::runtime::Runtime;

use crate::runtime_handles::RuntimeHandles;

const SETTLE: Duration = Duration::from_millis(20);
const TIMER_STOP_BUDGET: Duration = Duration::from_millis(10);
const SCHEDULED_RUNS_STOP_BUDGET: Duration = Duration::from_millis(10);
const SERVER_STOP_BUDGET: Duration = Duration::from_millis(40);
const SPEAK_STOP_BUDGET: Duration = Duration::from_millis(20);
const FLUSH_BUDGET: Duration = Duration::from_millis(60);
const ABANDON_BUDGET: Duration = Duration::from_millis(10);
const STORAGE_CLOSE_BUDGET: Duration = Duration::from_millis(20);
const GRACEFUL_BUDGET: Duration = TIMER_STOP_BUDGET
    .saturating_add(SCHEDULED_RUNS_STOP_BUDGET)
    .saturating_add(SETTLE)
    .saturating_add(SERVER_STOP_BUDGET)
    .saturating_add(SPEAK_STOP_BUDGET)
    .saturating_add(FLUSH_BUDGET)
    .saturating_add(ABANDON_BUDGET)
    .saturating_add(STORAGE_CLOSE_BUDGET);

const _: () = assert!(GRACEFUL_BUDGET.as_millis() < gpui::SHUTDOWN_TIMEOUT.as_millis());

pub const RUNTIME_SHUTDOWN_BUDGET: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct RuntimeSlot(Rc<RefCell<Option<Runtime>>>);

impl Global for RuntimeSlot {}

impl RuntimeSlot {
    pub fn new(runtime: Runtime) -> Self {
        Self(Rc::new(RefCell::new(Some(runtime))))
    }

    pub fn take(&self) -> Option<Runtime> {
        self.0.borrow_mut().take()
    }
}

pub fn silence_logging() {
    log::set_max_level(log::LevelFilter::Off);
}

pub struct QuitTeardown(Option<Runtime>);

impl QuitTeardown {
    pub fn claim(cx: &App) -> Self {
        if cfg!(target_os = "macos") {
            Self(cx.try_global::<RuntimeSlot>().and_then(RuntimeSlot::take))
        } else {
            Self(None)
        }
    }

    pub fn finish(self) {
        let Some(runtime) = self.0 else {
            return;
        };
        let _ =
            std::thread::spawn(move || runtime.shutdown_timeout(RUNTIME_SHUTDOWN_BUDGET)).join();
        silence_logging();
    }
}

pub struct ShutdownHandles {
    bus: Arc<EventBus>,
    action_engine: ActionEngineHandle,
    scheduler: QueueSchedulerHandle,
    trigger_evaluator: TriggerEvaluatorHandle,
    timer_scheduler: TimerSchedulerHandle,
    scheduled_runs: ScheduledRunsHandle,
    server: Option<ServerHandle>,
    speak: Option<SpeakQueueHandle>,
    hotkey: Option<Arc<forge_hotkey::HotkeyClient>>,
    storage: Arc<dyn DataProvider>,
}

impl ShutdownHandles {
    pub fn from_handles(handles: &RuntimeHandles) -> Self {
        Self {
            bus: Arc::clone(&handles.bus),
            action_engine: handles.action_engine.clone(),
            scheduler: handles.scheduler.clone(),
            trigger_evaluator: handles.trigger_evaluator.clone(),
            timer_scheduler: handles.timer_scheduler.clone(),
            scheduled_runs: handles.scheduled_runs.clone(),
            server: handles.server.clone(),
            speak: handles.speak.clone(),
            hotkey: handles.hotkey_client.clone(),
            storage: Arc::clone(&handles.backend),
        }
    }

    pub async fn run_graceful(self) {
        let _ = tokio::time::timeout(GRACEFUL_BUDGET, self.sequence()).await;
    }

    async fn sequence(self) {
        if let Some(hotkey) = &self.hotkey {
            hotkey.release_open_holds();
        }

        tracing::info!("graceful shutdown: stopping intake");
        let _ = tokio::time::timeout(TIMER_STOP_BUDGET, self.timer_scheduler.stop()).await;
        let _ = tokio::time::timeout(SCHEDULED_RUNS_STOP_BUDGET, self.scheduled_runs.stop()).await;
        self.trigger_evaluator.shutdown();

        if let Some(server) = self.server {
            let _ = tokio::time::timeout(SERVER_STOP_BUDGET, server.stop()).await;
        }

        tokio::time::sleep(SETTLE).await;

        self.scheduler.shutdown();
        self.action_engine.shutdown();

        if let Some(speak) = self.speak {
            let _ = tokio::time::timeout(SPEAK_STOP_BUDGET, speak.send(SpeakCommand::Clear)).await;
        }

        self.bus.shutdown();
        if tokio::time::timeout(FLUSH_BUDGET, self.bus.await_flush())
            .await
            .is_ok()
        {
            tracing::info!("graceful shutdown: event log flushed");
        } else {
            match tokio::time::timeout(ABANDON_BUDGET, self.bus.abandon_flush()).await {
                Ok(rows) => tracing::warn!(
                    rows,
                    "graceful shutdown: flush budget ran out; queued rows left unwritten"
                ),
                Err(_) => tracing::warn!(
                    "graceful shutdown: flush budget ran out; unwritten rows could not be counted"
                ),
            }
        }

        let _ = tokio::time::timeout(STORAGE_CLOSE_BUDGET, self.storage.shutdown()).await;
        tracing::info!("graceful shutdown: storage closed");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::io::Write;
    use std::sync::Mutex;

    use forge_events::{Event, EventSource};
    use forge_registry::{SubActionRegistry, TriggerRegistry};
    use forge_runtime::{
        ActionCancelRegistry, CatchUpSettle, Config, QueueScheduler, ScheduledRunsParts,
        SystemWallClock, spawn_action_engine, spawn_live_viewer_aggregator, spawn_scheduled_runs,
        spawn_stream_live_signal, spawn_timer_scheduler, spawn_trigger_evaluator,
    };
    use forge_storage::action::MockActionRepo;
    use forge_storage::history::MockHistoryRepo;
    use forge_storage::{CatalogRevision, EventLogRepo, MockScheduledRunRepo, StorageError};
    use forge_types::EventId;
    use time::OffsetDateTime;

    use super::*;
    use crate::test_support::{stub_catalog, test_backend};

    struct StuckEventLog;

    #[async_trait::async_trait]
    impl EventLogRepo for StuckEventLog {
        async fn insert(&self, _: &Event) -> Result<(), StorageError> {
            std::future::pending().await
        }

        async fn get(&self, _: EventId) -> Result<Option<Event>, StorageError> {
            Ok(None)
        }

        async fn recent(&self, _: usize) -> Result<Vec<Event>, StorageError> {
            Ok(Vec::new())
        }

        async fn recent_since(
            &self,
            _: usize,
            _: Option<EventId>,
        ) -> Result<Vec<Event>, StorageError> {
            Ok(Vec::new())
        }

        async fn prune_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
            Ok(0)
        }
    }

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn lines_containing(&self, needle: &str) -> Vec<String> {
            String::from_utf8(self.0.lock().unwrap().clone())
                .unwrap()
                .lines()
                .filter(|line| line.contains(needle))
                .map(str::to_owned)
                .collect()
        }
    }

    fn handles_over(bus: Arc<EventBus>) -> ShutdownHandles {
        let catalog = stub_catalog();
        let action_engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&catalog),
            Arc::new(MockActionRepo::new()),
            Arc::new(MockHistoryRepo::new()),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        );
        let scheduler = QueueScheduler::spawn(action_engine.clone(), Arc::clone(&bus), Vec::new());
        let trigger_evaluator = spawn_trigger_evaluator(
            Arc::clone(&bus),
            Arc::new(TriggerRegistry::new()),
            Arc::clone(&catalog),
            scheduler.clone(),
            Config::default(),
        );
        let timer_scheduler = spawn_timer_scheduler(
            Arc::clone(&bus),
            Arc::clone(&catalog),
            spawn_stream_live_signal(
                &spawn_live_viewer_aggregator(),
                forge_obs::SwitchableObsSink::new().stream_output(),
            ),
            forge_types::Shared::default(),
        );
        let mut run_repo = MockScheduledRunRepo::new();
        run_repo.expect_next_due().returning(|| Ok(None));
        let scheduled_runs = spawn_scheduled_runs(ScheduledRunsParts {
            repo: Arc::new(run_repo),
            revision: CatalogRevision::new(),
            catalog: Arc::clone(&catalog),
            actions: Arc::new(MockActionRepo::new()),
            queues: scheduler.clone(),
            bus: Arc::clone(&bus),
            clock: Arc::new(SystemWallClock),
            catch_up: CatchUpSettle::immediately(),
        });
        ShutdownHandles {
            timer_scheduler,
            scheduled_runs,
            bus,
            action_engine,
            scheduler,
            trigger_evaluator,
            server: None,
            speak: None,
            hotkey: None,
            storage: test_backend().0,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_flush_that_outlasts_its_budget_logs_the_rows_left_unwritten_once() {
        let captured = Captured::default();
        let sink = captured.clone();
        let _logs = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_ansi(false)
                .with_writer(move || sink.clone())
                .finish(),
        );
        let bus = EventBus::new(Arc::new(StuckEventLog));
        EventBus::spawn_flush_task(Arc::clone(&bus));
        let handles = handles_over(Arc::clone(&bus));
        for kind in ["a", "b", "c"] {
            bus.publish(Event::new(EventSource::Core, kind, serde_json::Value::Null));
        }

        handles.run_graceful().await;

        let lines = captured.lines_containing("flush budget ran out");
        assert!(
            matches!(lines.as_slice(), [only] if only.contains("rows=3")),
            "{lines:?}"
        );
    }

    fn teardown_with_stuck_blocking_task() -> (QuitTeardown, std::sync::mpsc::Sender<()>) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let (release, parked) = std::sync::mpsc::channel::<()>();
        let (started_tx, started_rx) = std::sync::mpsc::channel::<()>();
        runtime.spawn_blocking(move || {
            started_tx.send(()).unwrap();
            let _ = parked.recv();
        });
        started_rx.recv().unwrap();
        (QuitTeardown(Some(runtime)), release)
    }

    #[test]
    fn finish_returns_within_the_runtime_budget_despite_a_stuck_blocking_task_and_silences_logging()
    {
        let (teardown, release) = teardown_with_stuck_blocking_task();
        log::set_max_level(log::LevelFilter::Info);
        let started = std::time::Instant::now();

        teardown.finish();

        let elapsed = started.elapsed();
        release.send(()).unwrap();
        assert!(
            elapsed < RUNTIME_SHUTDOWN_BUDGET + Duration::from_secs(2),
            "{elapsed:?}"
        );
        assert_eq!(log::max_level(), log::LevelFilter::Off);
    }

    #[test]
    fn finish_without_a_claimed_runtime_returns_immediately() {
        let started = std::time::Instant::now();

        QuitTeardown(None).finish();

        assert!(started.elapsed() < RUNTIME_SHUTDOWN_BUDGET);
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

    #[test]
    fn every_step_budget_up_to_the_unwritten_row_count_fits_inside_the_graceful_budget() {
        let worst_case =
            SETTLE + SERVER_STOP_BUDGET + SPEAK_STOP_BUDGET + FLUSH_BUDGET + ABANDON_BUDGET;

        assert!(
            worst_case <= GRACEFUL_BUDGET,
            "{worst_case:?} of steps before the count is logged, {GRACEFUL_BUDGET:?} allowed"
        );
    }
}
