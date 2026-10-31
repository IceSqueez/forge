use std::sync::Arc;
use std::time::Duration;

use forge_net::{TaskOwner, TaskShutdown};
use forge_runtime::{
    ActionEngineHandle, EventBus, IntegrationSupervisor, QueueSchedulerHandle, ScheduledRunsHandle,
    TimerSchedulerHandle, TriggerEvaluatorHandle,
};
use forge_server::ServerHandle;
use forge_speak_queue::{SpeakCommand, SpeakQueueHandle};
use forge_storage::DataProvider;
use forge_transfer::{ScheduleState, ScheduledBackupHandle};

use crate::integrations::BuiltinObject;

const SETTLE: Duration = Duration::from_millis(2);
const TIMER_STOP_BUDGET: Duration = Duration::from_millis(10);
const SCHEDULED_RUNS_STOP_BUDGET: Duration = Duration::from_millis(10);
const TRIGGER_EVALUATOR_STOP_BUDGET: Duration = Duration::from_millis(10);
const SERVER_STOP_BUDGET: Duration = Duration::from_millis(30);
const RUNS_STOP_BUDGET: Duration = Duration::from_millis(7);
const SERVICES_STOP_BUDGET: Duration = Duration::from_millis(3);
const INTEGRATIONS_STOP_BUDGET: Duration = Duration::from_millis(25);
const SPEAK_STOP_BUDGET: Duration = Duration::from_millis(10);
const FLUSH_BUDGET: Duration = Duration::from_millis(60);
const ABANDON_BUDGET: Duration = Duration::from_millis(10);
const STORAGE_CLOSE_BUDGET: Duration = Duration::from_millis(20);
pub const GRACEFUL_BUDGET: Duration = TIMER_STOP_BUDGET
    .saturating_add(SCHEDULED_RUNS_STOP_BUDGET)
    .saturating_add(TRIGGER_EVALUATOR_STOP_BUDGET)
    .saturating_add(SETTLE)
    .saturating_add(SERVER_STOP_BUDGET)
    .saturating_add(RUNS_STOP_BUDGET)
    .saturating_add(SERVICES_STOP_BUDGET)
    .saturating_add(INTEGRATIONS_STOP_BUDGET)
    .saturating_add(SPEAK_STOP_BUDGET)
    .saturating_add(FLUSH_BUDGET)
    .saturating_add(ABANDON_BUDGET)
    .saturating_add(STORAGE_CLOSE_BUDGET);

const SCHEDULED_BACKUP_STOP_BUDGET: Duration = GRACEFUL_BUDGET;

async fn stop_scheduled_backups(scheduled: ScheduledBackupHandle) {
    scheduled.cancel_in_flight();
    scheduled.shutdown();
    let mut status = scheduled.status();
    let stopped = status.wait_for(|current| current.state != ScheduleState::Running);
    if tokio::time::timeout(SCHEDULED_BACKUP_STOP_BUDGET, stopped)
        .await
        .is_err()
    {
        tracing::warn!("graceful shutdown: scheduled backup did not stop within its budget");
    }
}

pub struct OrderedStopParts {
    pub bus: Arc<EventBus>,
    pub action_engine: ActionEngineHandle,
    pub scheduler: QueueSchedulerHandle,
    pub trigger_evaluator: TriggerEvaluatorHandle,
    pub timer_scheduler: TimerSchedulerHandle,
    pub scheduled_runs: ScheduledRunsHandle,
    pub server: Option<ServerHandle>,
    pub speak: Option<SpeakQueueHandle>,
    pub hotkey: Option<Arc<forge_hotkey::HotkeyClient>>,
    pub integrations: IntegrationSupervisor<BuiltinObject>,
    pub services: TaskOwner,
    pub storage: Arc<dyn DataProvider>,
    pub scheduled_backups: ScheduledBackupHandle,
}

#[derive(Clone)]
pub struct OrderedStop {
    sequence: StopSequence,
    scheduled_backups: ScheduledBackupHandle,
}

impl OrderedStop {
    pub fn new(parts: OrderedStopParts) -> Self {
        let OrderedStopParts {
            bus,
            action_engine,
            scheduler,
            trigger_evaluator,
            timer_scheduler,
            scheduled_runs,
            server,
            speak,
            hotkey,
            integrations,
            services,
            storage,
            scheduled_backups,
        } = parts;
        Self {
            sequence: StopSequence {
                bus,
                action_engine,
                scheduler,
                trigger_evaluator,
                timer_scheduler,
                scheduled_runs,
                server,
                speak,
                hotkey,
                integrations: Some(integrations),
                services,
                storage,
            },
            scheduled_backups,
        }
    }

    pub async fn run(self) {
        tokio::join!(
            stop_scheduled_backups(self.scheduled_backups),
            self.sequence.run_graceful(),
        );
    }
}

#[derive(Clone)]
struct StopSequence {
    bus: Arc<EventBus>,
    action_engine: ActionEngineHandle,
    scheduler: QueueSchedulerHandle,
    trigger_evaluator: TriggerEvaluatorHandle,
    timer_scheduler: TimerSchedulerHandle,
    scheduled_runs: ScheduledRunsHandle,
    server: Option<ServerHandle>,
    speak: Option<SpeakQueueHandle>,
    hotkey: Option<Arc<forge_hotkey::HotkeyClient>>,
    integrations: Option<IntegrationSupervisor<BuiltinObject>>,
    services: TaskOwner,
    storage: Arc<dyn DataProvider>,
}

impl StopSequence {
    async fn run_graceful(self) {
        if tokio::time::timeout(GRACEFUL_BUDGET, self.sequence())
            .await
            .is_err()
        {
            tracing::warn!(budget = ?GRACEFUL_BUDGET, "graceful shutdown ran out of time");
        }
    }

    async fn sequence(self) {
        if let Some(hotkey) = &self.hotkey {
            hotkey.release_open_holds();
        }

        tracing::info!("graceful shutdown: stopping intake");
        report_stop(
            "timer scheduler",
            self.timer_scheduler.shutdown(TIMER_STOP_BUDGET).await,
        );
        report_stop(
            "scheduled runs",
            self.scheduled_runs.stop(SCHEDULED_RUNS_STOP_BUDGET).await,
        );
        report_stop(
            "trigger evaluator",
            self.trigger_evaluator
                .shutdown(TRIGGER_EVALUATOR_STOP_BUDGET)
                .await,
        );

        if let Some(server) = self.server {
            match tokio::time::timeout(SERVER_STOP_BUDGET, server.stop()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::warn!(%error, "server stop failed"),
                Err(_) => {
                    tracing::warn!(budget = ?SERVER_STOP_BUDGET, "server did not stop in time")
                }
            }
        }

        tokio::time::sleep(SETTLE).await;

        self.scheduler.shutdown();
        self.action_engine.shutdown();
        let (queues, engine) = tokio::join!(
            self.scheduler.stopped(RUNS_STOP_BUDGET),
            self.action_engine.stopped(RUNS_STOP_BUDGET),
        );
        report_stop("queue scheduler", queues);
        report_stop("action engine", engine);

        if let Some(integrations) = &self.integrations {
            integrations.shutdown(INTEGRATIONS_STOP_BUDGET).await;
        }

        if let Some(speak) = self.speak {
            match tokio::time::timeout(SPEAK_STOP_BUDGET, speak.send(SpeakCommand::Clear)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::warn!(%error, "speak queue could not be cleared"),
                Err(_) => {
                    tracing::warn!(budget = ?SPEAK_STOP_BUDGET, "speak queue did not clear in time")
                }
            }
        }

        report_stop(
            "runtime services",
            self.services.shutdown(SERVICES_STOP_BUDGET).await,
        );

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

        if tokio::time::timeout(STORAGE_CLOSE_BUDGET, self.storage.shutdown())
            .await
            .is_ok()
        {
            tracing::info!("graceful shutdown: storage closed");
        } else {
            tracing::warn!(budget = ?STORAGE_CLOSE_BUDGET, "storage did not close in time");
        }
    }
}

fn report_stop(subsystem: &'static str, outcome: TaskShutdown) {
    if let TaskShutdown::Aborted { tasks } = outcome {
        tracing::warn!(
            subsystem,
            tasks,
            "tasks did not stop in time and were dropped"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_events::{Event, EventSource};
    use forge_registry::{SubActionRegistry, TriggerRegistry};
    use forge_runtime::TIMER_TICK_KIND;
    use forge_runtime::{
        ActionCancelRegistry, Catalog, CatchUpSettle, Config, HandOff, QueueScheduler,
        RuntimeError, ScheduledRunsParts, spawn_action_engine, spawn_live_viewer_aggregator,
        spawn_scheduled_runs, spawn_stream_live_signal, spawn_timer_scheduler,
        spawn_trigger_evaluator,
    };
    use forge_storage::MockActionRepo;
    use forge_storage::MockHistoryRepo;
    use forge_storage::MockTriggerInstanceRepo;
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

    fn handles_over(bus: Arc<EventBus>, intake: Intake) -> StopSequence {
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
                &spawn_live_viewer_aggregator(&forge_net::TaskOwner::new()),
                forge_obs::SwitchableObsSink::new()
                    .stream_output()
                    .into_receiver(),
                &TaskOwner::new(),
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
            clock: Arc::new(forge_types::SystemWallClock),
            catch_up: CatchUpSettle::immediately(),
        });
        StopSequence {
            timer_scheduler,
            scheduled_runs,
            bus,
            action_engine,
            scheduler,
            trigger_evaluator,
            server: None,
            speak: None,
            hotkey: None,
            integrations: None,
            services: TaskOwner::new(),
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
            crate::test_support::install_tracing_interest_floor();
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
                (Ok(HandOff::NotPending), Err(RuntimeError::SchedulerStopped))
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

    const RUNNING_DEADLINE: Duration = Duration::from_secs(600);

    async fn until_running(handle: &ScheduledBackupHandle) {
        let mut status = handle.status();
        tokio::time::timeout(
            RUNNING_DEADLINE,
            status.wait_for(|status| status.state == ScheduleState::Running),
        )
        .await
        .expect("the gated backup never reported Running")
        .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_scheduled_backups_returns_once_the_cancelled_run_ends_inside_the_budget() {
        let schedule = crate::test_support::gated_running_schedule();
        until_running(&schedule.handle).await;
        let started = tokio::time::Instant::now();

        let run_ends = async {
            tokio::task::yield_now().await;
            schedule.export_gate.notify_one();
        };
        tokio::join!(stop_scheduled_backups(schedule.handle.clone()), run_ends);

        let state = schedule.handle.status().borrow().state;
        assert!(
            state != ScheduleState::Running && started.elapsed() < SCHEDULED_BACKUP_STOP_BUDGET,
            "{state:?} after {:?}",
            started.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_scheduled_backups_gives_up_at_the_budget_when_the_run_never_ends() {
        let schedule = crate::test_support::gated_running_schedule();
        until_running(&schedule.handle).await;
        let started = tokio::time::Instant::now();

        stop_scheduled_backups(schedule.handle.clone()).await;

        let state = schedule.handle.status().borrow().state;
        assert!(
            state == ScheduleState::Running && started.elapsed() >= SCHEDULED_BACKUP_STOP_BUDGET,
            "{state:?} after {:?}",
            started.elapsed()
        );
    }

    struct RetiringIntegration {
        id: forge_types::IntegrationId,
        teardown_hangs: bool,
        on_teardown: Arc<dyn Fn() + Send + Sync>,
    }

    #[async_trait::async_trait]
    impl forge_runtime::IntegrationFactory<crate::integrations::BuiltinObject> for RetiringIntegration {
        fn id(&self) -> forge_types::IntegrationId {
            self.id.clone()
        }

        fn teardown_grace(&self) -> Duration {
            forge_runtime::DEFAULT_TEARDOWN_GRACE
        }

        async fn is_configured(&self) -> Result<bool, StorageError> {
            Ok(true)
        }

        async fn start(
            &self,
        ) -> Result<
            forge_runtime::RunningIntegration<crate::integrations::BuiltinObject>,
            forge_runtime::IntegrationStartError,
        > {
            let hangs = self.teardown_hangs;
            let on_teardown = Arc::clone(&self.on_teardown);
            Ok(
                forge_runtime::RunningIntegration::idle().with_teardown(Box::pin(async move {
                    on_teardown();
                    if hangs {
                        std::future::pending::<()>().await;
                    }
                })),
            )
        }
    }

    struct StalledRelayIntegration;

    #[async_trait::async_trait]
    impl forge_runtime::IntegrationFactory<crate::integrations::BuiltinObject>
        for StalledRelayIntegration
    {
        fn id(&self) -> forge_types::IntegrationId {
            forge_types::IntegrationId::from_static("kick")
        }

        fn teardown_grace(&self) -> Duration {
            forge_runtime::DEFAULT_TEARDOWN_GRACE
        }

        async fn is_configured(&self) -> Result<bool, StorageError> {
            Ok(true)
        }

        async fn start(
            &self,
        ) -> Result<
            forge_runtime::RunningIntegration<crate::integrations::BuiltinObject>,
            forge_runtime::IntegrationStartError,
        > {
            Ok(forge_runtime::RunningIntegration::idle().with_teardown(
                crate::integration_factories::platform_teardown(
                    forge_runtime::TaskGroup::default(),
                    crate::integrations::EventRelay::stalled(Arc::new(())),
                    std::future::ready(()),
                ),
            ))
        }
    }

    async fn running_integration(
        settings: Arc<dyn forge_storage::SettingsRepo>,
        teardown_hangs: bool,
        on_teardown: impl Fn() + Send + Sync + 'static,
    ) -> IntegrationSupervisor<crate::integrations::BuiltinObject> {
        let id = forge_types::IntegrationId::from_static("twitch");
        let supervisor = IntegrationSupervisor::launch(
            vec![Arc::new(RetiringIntegration {
                id: id.clone(),
                teardown_hangs,
                on_teardown: Arc::new(on_teardown),
            })
                as Arc<
                    dyn forge_runtime::IntegrationFactory<crate::integrations::BuiltinObject>,
                >],
            settings,
            forge_runtime::IntegrationGate::new(),
            Arc::new(crate::integrations::BuiltinRegistry::default()),
            spawn_live_viewer_aggregator(&forge_net::TaskOwner::new()),
        );
        supervisor
            .slot(&id)
            .unwrap()
            .set_enabled(true)
            .await
            .unwrap();
        supervisor
    }

    type Milestones = Arc<Mutex<Vec<&'static str>>>;

    fn milestone(log: &Milestones, name: &'static str) -> impl Fn() + Send + Sync + 'static {
        let log = Arc::clone(log);
        move || log.lock().unwrap().push(name)
    }

    fn settings_store() -> Arc<dyn forge_storage::SettingsRepo> {
        Arc::new(forge_storage::test_support::MemorySettings::new())
    }

    #[tokio::test(start_paused = true)]
    async fn runtime_services_end_before_storage_closes() {
        let milestones = Milestones::default();
        let bus = EventBus::new(Arc::new(RecordedEventLog::default()));
        EventBus::spawn_flush_task(Arc::clone(&bus));
        let services = TaskOwner::new();
        let stopped = milestone(&milestones, "services stopped");
        services.spawn({
            let services = services.clone();
            async move {
                services.stopped().await;
                stopped();
            }
        });
        let handles = StopSequence {
            services,
            storage: crate::test_support::test_backend_observing_shutdown(milestone(
                &milestones,
                "storage closed",
            )),
            ..handles_over(Arc::clone(&bus), Intake::idle())
        };

        handles.run_graceful().await;

        assert_eq!(
            *milestones.lock().unwrap(),
            ["services stopped", "storage closed"]
        );
    }

    #[tokio::test(start_paused = true)]
    async fn integrations_retire_before_storage_closes() {
        let milestones = Milestones::default();
        let bus = EventBus::new(Arc::new(RecordedEventLog::default()));
        EventBus::spawn_flush_task(Arc::clone(&bus));
        let integrations =
            running_integration(settings_store(), false, milestone(&milestones, "teardown")).await;
        let handles = StopSequence {
            integrations: Some(integrations),
            storage: crate::test_support::test_backend_observing_shutdown(milestone(
                &milestones,
                "storage closed",
            )),
            ..handles_over(Arc::clone(&bus), Intake::idle())
        };

        handles.run_graceful().await;

        assert_eq!(*milestones.lock().unwrap(), ["teardown", "storage closed"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_hung_integration_costs_its_budget_and_storage_still_closes() {
        let milestones = Milestones::default();
        let bus = EventBus::new(Arc::new(RecordedEventLog::default()));
        EventBus::spawn_flush_task(Arc::clone(&bus));
        let integrations =
            running_integration(settings_store(), true, milestone(&milestones, "teardown")).await;
        let handles = StopSequence {
            integrations: Some(integrations),
            storage: crate::test_support::test_backend_observing_shutdown(milestone(
                &milestones,
                "storage closed",
            )),
            ..handles_over(Arc::clone(&bus), Intake::idle())
        };

        handles.run_graceful().await;

        assert_eq!(*milestones.lock().unwrap(), ["teardown", "storage closed"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_stalled_event_relay_retires_inside_the_integrations_stop_budget() {
        let factory = StalledRelayIntegration;
        let id = forge_runtime::IntegrationFactory::id(&factory);
        let supervisor = IntegrationSupervisor::launch(
            vec![Arc::new(factory)
                as Arc<
                    dyn forge_runtime::IntegrationFactory<crate::integrations::BuiltinObject>,
                >],
            settings_store(),
            forge_runtime::IntegrationGate::new(),
            Arc::new(crate::integrations::BuiltinRegistry::default()),
            spawn_live_viewer_aggregator(&forge_net::TaskOwner::new()),
        );
        supervisor
            .slot(&id)
            .unwrap()
            .set_enabled(true)
            .await
            .unwrap();

        supervisor.shutdown(INTEGRATIONS_STOP_BUDGET).await;

        assert_eq!(
            supervisor.state_of(&id),
            forge_runtime::LifecycleState::Disabled
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_event_published_by_a_retiring_integration_is_flushed() {
        const FAREWELL: &str = "integration.farewell";
        let log = Arc::new(RecordedEventLog::default());
        let bus = EventBus::new(Arc::clone(&log) as Arc<dyn EventLogRepo>);
        EventBus::spawn_flush_task(Arc::clone(&bus));
        let publisher = Arc::clone(&bus);
        let integrations = running_integration(settings_store(), false, move || {
            publisher.publish(Event::new(
                EventSource::Core,
                FAREWELL,
                serde_json::Value::Null,
            ));
        })
        .await;
        let handles = StopSequence {
            integrations: Some(integrations),
            ..handles_over(Arc::clone(&bus), Intake::idle())
        };

        handles.run_graceful().await;

        assert!(log.kinds().iter().any(|kind| kind == FAREWELL));
    }
}
