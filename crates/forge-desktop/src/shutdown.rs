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

const SETTLE: Duration = Duration::from_millis(10);
const TIMER_STOP_BUDGET: Duration = Duration::from_millis(10);
const SCHEDULED_RUNS_STOP_BUDGET: Duration = Duration::from_millis(10);
const TRIGGER_EVALUATOR_STOP_BUDGET: Duration = Duration::from_millis(10);
const SERVER_STOP_BUDGET: Duration = Duration::from_millis(40);
const SPEAK_STOP_BUDGET: Duration = Duration::from_millis(20);
const FLUSH_BUDGET: Duration = Duration::from_millis(60);
const ABANDON_BUDGET: Duration = Duration::from_millis(10);
const STORAGE_CLOSE_BUDGET: Duration = Duration::from_millis(20);
const GRACEFUL_BUDGET: Duration = TIMER_STOP_BUDGET
    .saturating_add(SCHEDULED_RUNS_STOP_BUDGET)
    .saturating_add(TRIGGER_EVALUATOR_STOP_BUDGET)
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
        let _ = tokio::time::timeout(TRIGGER_EVALUATOR_STOP_BUDGET, self.trigger_evaluator.stop())
            .await;

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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_events::{Event, EventSource};
    use forge_registry::{SubActionRegistry, TriggerRegistry};
    use forge_runtime::triggers::TIMER_TICK_KIND;
    use forge_runtime::{
        ActionCancelRegistry, Catalog, CatchUpSettle, Config, HandOff, QueueScheduler,
        ScheduleError, ScheduledRunsParts, SystemWallClock, spawn_action_engine,
        spawn_live_viewer_aggregator, spawn_scheduled_runs, spawn_stream_live_signal,
        spawn_timer_scheduler, spawn_trigger_evaluator,
    };
    use forge_storage::action::MockActionRepo;
    use forge_storage::history::MockHistoryRepo;
    use forge_storage::trigger_instance::MockTriggerInstanceRepo;
    use forge_storage::{
        ActionRepo, ActionTelemetry, CatalogRevision, EventLogRepo, ExecutionStatus,
        MockScheduledRunRepo, ScheduledRun, ScheduledRunId, ScheduledRunPlacement,
        ScheduledRunRepo, ScheduledRunSpec, StorageError,
    };
    use forge_types::{
        Action, ActionId, EventId, ExecutionMode, PermissionRung, PlatformScope, QueueId,
        TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
    };
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

    #[derive(Default)]
    struct RecordedEventLog(Mutex<Vec<String>>);

    impl RecordedEventLog {
        fn kinds(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl EventLogRepo for RecordedEventLog {
        async fn insert(&self, event: &Event) -> Result<(), StorageError> {
            self.0.lock().unwrap().push(event.kind.clone());
            Ok(())
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
    }

    struct StalledActions;

    #[async_trait::async_trait]
    impl ActionRepo for StalledActions {
        async fn list(&self) -> Result<Vec<Action>, StorageError> {
            std::future::pending().await
        }

        async fn get(&self, _: ActionId) -> Result<Option<Action>, StorageError> {
            std::future::pending().await
        }

        async fn save(&self, _: &Action) -> Result<(), StorageError> {
            std::future::pending().await
        }

        async fn delete(&self, _: ActionId) -> Result<bool, StorageError> {
            std::future::pending().await
        }

        async fn telemetry(&self, _: ActionId) -> Result<ActionTelemetry, StorageError> {
            std::future::pending().await
        }

        async fn record_execution(
            &self,
            _: ActionId,
            _: OffsetDateTime,
            _: u64,
            _: ExecutionStatus,
        ) -> Result<(), StorageError> {
            std::future::pending().await
        }

        async fn prune_executions_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
            std::future::pending().await
        }
    }

    struct StalledRuns;

    #[async_trait::async_trait]
    impl ScheduledRunRepo for StalledRuns {
        async fn schedule(
            &self,
            _: &ScheduledRunSpec,
        ) -> Result<ScheduledRunPlacement, StorageError> {
            std::future::pending().await
        }

        async fn get(&self, _: ScheduledRunId) -> Result<Option<ScheduledRun>, StorageError> {
            std::future::pending().await
        }

        async fn list_pending(&self) -> Result<Vec<ScheduledRun>, StorageError> {
            std::future::pending().await
        }

        async fn list_due(&self, _: OffsetDateTime) -> Result<Vec<ScheduledRun>, StorageError> {
            std::future::pending().await
        }

        async fn fail_unreadable_due(
            &self,
            _: OffsetDateTime,
            _: &str,
        ) -> Result<Vec<ScheduledRunId>, StorageError> {
            std::future::pending().await
        }

        async fn next_due(&self) -> Result<Option<OffsetDateTime>, StorageError> {
            std::future::pending().await
        }

        async fn claim(
            &self,
            _: ScheduledRunId,
            _: OffsetDateTime,
        ) -> Result<Option<ScheduledRun>, StorageError> {
            std::future::pending().await
        }

        async fn cancel(&self, _: ScheduledRunId, _: OffsetDateTime) -> Result<bool, StorageError> {
            std::future::pending().await
        }

        async fn cancel_by_key(&self, _: &str, _: OffsetDateTime) -> Result<bool, StorageError> {
            std::future::pending().await
        }

        async fn settle(
            &self,
            _: ScheduledRunId,
            _: forge_storage::ScheduledRunOutcome,
            _: Option<String>,
            _: OffsetDateTime,
        ) -> Result<bool, StorageError> {
            std::future::pending().await
        }

        async fn list_recent_resolved(&self, _: usize) -> Result<Vec<ScheduledRun>, StorageError> {
            std::future::pending().await
        }

        async fn prune_resolved_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
            std::future::pending().await
        }

        async fn count_pending(&self) -> Result<u64, StorageError> {
            std::future::pending().await
        }

        async fn count_pending_for_action(&self, _: ActionId) -> Result<u64, StorageError> {
            std::future::pending().await
        }
    }

    const TIMER_MINUTES: u64 = 10;
    const TIMER_INTERVAL: Duration = Duration::from_secs(TIMER_MINUTES * 60);

    struct Intake {
        timer_catalog: Arc<Catalog>,
        runs: Arc<dyn ScheduledRunRepo>,
        evaluator_catalog: Arc<Catalog>,
    }

    fn stalled_catalog() -> Arc<Catalog> {
        Catalog::new(
            Arc::new(StalledActions),
            Arc::new(MockTriggerInstanceRepo::new()),
            CatalogRevision::new(),
        )
    }

    impl Intake {
        fn idle() -> Self {
            let mut runs = MockScheduledRunRepo::new();
            runs.expect_next_due().returning(|| Ok(None));
            runs.expect_get().returning(|_| Ok(None));
            Self {
                timer_catalog: stub_catalog(),
                runs: Arc::new(runs),
                evaluator_catalog: stub_catalog(),
            }
        }

        fn stalled() -> Self {
            Self {
                timer_catalog: stalled_catalog(),
                runs: Arc::new(StalledRuns),
                evaluator_catalog: stalled_catalog(),
            }
        }

        fn with_stalled_timer(self) -> Self {
            Self {
                timer_catalog: stalled_catalog(),
                ..self
            }
        }

        fn with_stalled_runs(self) -> Self {
            Self {
                runs: Arc::new(StalledRuns),
                ..self
            }
        }

        fn with_evaluator_catalog(self, evaluator_catalog: Arc<Catalog>) -> Self {
            Self {
                evaluator_catalog,
                ..self
            }
        }

        fn with_armed_timer(self) -> Self {
            let action = Action {
                id: ActionId::new(),
                name: "timed".to_owned(),
                group: None,
                queue_id: QueueId::new(),
                enabled: true,
                concurrent: false,
                bypass_pause: false,
                execution_mode: ExecutionMode::Sequential,
                description: None,
                sub_actions: vec![],
            };
            let mut overrides = TriggerConfig::new();
            overrides.insert(
                "interval_minutes".to_owned(),
                Variant::Int(i64::try_from(TIMER_MINUTES).unwrap()),
            );
            let timer = TriggerInstance {
                id: TriggerInstanceId::new(),
                kind_id: TIMER_TICK_KIND.to_owned(),
                name: "timer".to_owned(),
                overrides,
                enabled: true,
                user_defined: true,
                platform_scope: PlatformScope::Any,
                cooldown_secs: 0,
                cooldown_global: true,
                permission_rung: PermissionRung::Everyone,
            };
            let mut actions = MockActionRepo::new();
            actions
                .expect_list()
                .returning(move || Ok(vec![action.clone()]));
            let mut instances = MockTriggerInstanceRepo::new();
            instances
                .expect_list_for_action()
                .returning(move |_| Ok(vec![timer.clone()]));
            Self {
                timer_catalog: Catalog::new(
                    Arc::new(actions),
                    Arc::new(instances),
                    CatalogRevision::new(),
                ),
                ..self
            }
        }
    }

    fn handles_over(bus: Arc<EventBus>, intake: Intake) -> ShutdownHandles {
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
            intake.evaluator_catalog,
            scheduler.clone(),
            Config::default(),
        );
        let timer_scheduler = spawn_timer_scheduler(
            Arc::clone(&bus),
            intake.timer_catalog,
            spawn_stream_live_signal(
                &spawn_live_viewer_aggregator(),
                forge_obs::SwitchableObsSink::new().stream_output(),
            ),
            forge_types::Shared::default(),
        );
        let scheduled_runs = spawn_scheduled_runs(ScheduledRunsParts {
            repo: intake.runs,
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

    async fn settle() {
        for _ in 0..64 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_flush_that_outlasts_its_budget_logs_the_rows_left_unwritten_once_even_behind_stalled_intake()
     {
        for (intake, label) in [
            (Intake::idle as fn() -> Intake, "idle intake"),
            (
                Intake::stalled,
                "stalled timer, scheduled runs and trigger evaluator",
            ),
        ] {
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
            let handles = handles_over(Arc::clone(&bus), intake());
            for kind in ["a", "b", "c"] {
                bus.publish(Event::new(EventSource::Core, kind, serde_json::Value::Null));
            }

            handles.run_graceful().await;

            let lines = captured.lines_containing("flush budget ran out");
            assert!(
                matches!(lines.as_slice(), [only] if only.contains("rows=3")),
                "{label}: {lines:?}"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn scheduled_runs_keep_serving_until_the_timer_stop_finishes_or_runs_out_of_budget() {
        let bus = EventBus::new(Arc::new(RecordedEventLog::default()));
        let handles = handles_over(Arc::clone(&bus), Intake::idle().with_stalled_timer());
        let probe = handles.scheduled_runs.clone();
        settle().await;
        let shutting_down = tokio::spawn(handles.run_graceful());

        tokio::time::advance(TIMER_STOP_BUDGET / 2).await;
        settle().await;
        let during_timer_stop = probe.run_now(ScheduledRunId::new(1)).await;
        tokio::time::advance(TIMER_STOP_BUDGET).await;
        settle().await;
        let after_timer_stop = probe.run_now(ScheduledRunId::new(1)).await;

        assert!(
            matches!(
                (during_timer_stop, after_timer_stop),
                (
                    Ok(HandOff::NotPending),
                    Err(ScheduleError::SchedulerStopped)
                )
            ),
            "the scheduled runs were stopped before the timer scheduler"
        );
        shutting_down.abort();
    }

    struct EvaluationProbe {
        loads: Arc<AtomicUsize>,
        revision: CatalogRevision,
    }

    impl EvaluationProbe {
        fn new() -> (Self, Arc<Catalog>) {
            let loads = Arc::new(AtomicUsize::new(0));
            let counted = Arc::clone(&loads);
            let mut actions = MockActionRepo::new();
            actions.expect_list().returning(move || {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(Vec::new())
            });
            let revision = CatalogRevision::new();
            let catalog = Catalog::new(
                Arc::new(actions),
                Arc::new(MockTriggerInstanceRepo::new()),
                revision.clone(),
            );
            (Self { loads, revision }, catalog)
        }

        async fn evaluates(&self, bus: &EventBus) -> bool {
            let before = self.loads.load(Ordering::SeqCst);
            self.revision.advance();
            bus.publish(Event::new(
                EventSource::Core,
                "probe",
                serde_json::Value::Null,
            ));
            settle().await;
            self.loads.load(Ordering::SeqCst) > before
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_trigger_evaluator_keeps_evaluating_until_the_scheduled_runs_stop_runs_out_of_budget()
     {
        let (probe, evaluator_catalog) = EvaluationProbe::new();
        let bus = EventBus::new(Arc::new(RecordedEventLog::default()));
        let handles = handles_over(
            Arc::clone(&bus),
            Intake::idle()
                .with_stalled_runs()
                .with_evaluator_catalog(evaluator_catalog),
        );
        settle().await;
        let shutting_down = tokio::spawn(handles.run_graceful());

        tokio::time::advance(SCHEDULED_RUNS_STOP_BUDGET / 2).await;
        settle().await;
        let during_scheduled_runs_stop = probe.evaluates(&bus).await;
        tokio::time::advance(SCHEDULED_RUNS_STOP_BUDGET).await;
        settle().await;
        let after_scheduled_runs_stop = probe.evaluates(&bus).await;

        assert_eq!(
            (during_scheduled_runs_stop, after_scheduled_runs_stop),
            (true, false)
        );
        shutting_down.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_tick_fired_before_shutdown_is_flushed_and_none_fires_after() {
        let log = Arc::new(RecordedEventLog::default());
        let bus = EventBus::new(Arc::clone(&log) as Arc<dyn EventLogRepo>);
        EventBus::spawn_flush_task(Arc::clone(&bus));
        let handles = handles_over(Arc::clone(&bus), Intake::idle().with_armed_timer());
        let mut ticks = bus.subscribe();
        settle().await;
        tokio::time::advance(TIMER_INTERVAL).await;
        settle().await;

        handles.run_graceful().await;
        tokio::time::advance(TIMER_INTERVAL * 3).await;
        settle().await;

        let logged = log
            .kinds()
            .iter()
            .filter(|kind| kind.as_str() == TIMER_TICK_KIND)
            .count();
        let mut published = 0;
        while let Ok(Some(event)) = ticks.try_recv() {
            if event.kind == TIMER_TICK_KIND {
                published += 1;
            }
        }
        assert_eq!((logged, published), (1, 1));
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
        let worst_case = TIMER_STOP_BUDGET
            + SCHEDULED_RUNS_STOP_BUDGET
            + TRIGGER_EVALUATOR_STOP_BUDGET
            + SETTLE
            + SERVER_STOP_BUDGET
            + SPEAK_STOP_BUDGET
            + FLUSH_BUDGET
            + ABANDON_BUDGET;

        assert!(
            worst_case <= GRACEFUL_BUDGET,
            "{worst_case:?} of steps before the count is logged, {GRACEFUL_BUDGET:?} allowed"
        );
    }
}
