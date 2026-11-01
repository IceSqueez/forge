#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test fixtures"
)]
mod common;
use common::repos::InMemoryActionRepo;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::Event;
use forge_events::EventSubscription;
use forge_net::TaskShutdown;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
    SubActionRunner,
};
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, Catalog, EventBus, ExecutionRequest,
    MAX_PENDING_PER_QUEUE, NullEventLogRepo, PendingQuickAction, QueueAdmission, QueueMode,
    QueueRuntimeState, QueueScheduler, QueueSchedulerHandle, RuntimeError, SchedulerRequest,
    spawn_action_engine,
};
use forge_runtime::{CoreLogicWaitRunner, WAIT_KIND_ID, WAIT_MS_KEY};
use forge_storage::MockTriggerInstanceRepo;
use forge_storage::RunRefusal;
use forge_storage::{ActionRepo, ActionStats, CatalogRevision, HistoryRepo, StorageError};
use forge_types::{
    Action, ActionId, ArgStack, DEFAULT_QUEUE_ID, EventId, ExecutionContext, ExecutionMetadata,
    ExecutionOutcome, Queue, QueueId, SubActionConfig, SubActionOutcome, SubActionStep,
    SubActionTelemetry, Variant,
};
use time::OffsetDateTime;
use tokio::sync::{Notify, mpsc, watch};

const SLOW_KIND_ID: &str = "test.slow";
const INSTANT_KIND_ID: &str = "test.instant";
const PANIC_KIND_ID: &str = "test.panic";
const UNTIL_CANCELLED_KIND_ID: &str = "test.until_cancelled";
const FAILING_KIND_ID: &str = "test.fail";
const UNREGISTERED_KIND_ID: &str = "test.unregistered";
const FAILURE_REASON: &str = "scene not found";
const CANCELLED_REASON: &str = "saw the cancel";
const LONG_WAIT_MS: i64 = 60_000;
const SETTLE_ROUNDS: usize = 200;
const QUIET_STEP: Duration = Duration::from_secs(5);
const QUIET_ROUNDS: usize = 4;
const SAVE_DEADLINE: Duration = Duration::from_secs(2);

struct CapturingHistoryRepo {
    saved: mpsc::UnboundedSender<ExecutionContext>,
    gate: watch::Receiver<bool>,
}

#[async_trait]
impl HistoryRepo for CapturingHistoryRepo {
    async fn save(&self, ctx: &ExecutionContext) -> Result<(), StorageError> {
        let mut gate = self.gate.clone();
        #[expect(
            clippy::let_underscore_must_use,
            reason = "only completion matters here"
        )]
        let _ = gate.wait_for(|open| *open).await;
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = self.saved.send(ctx.clone());
        Ok(())
    }
    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(Vec::new())
    }
    async fn recent_for_builtin(
        &self,
        _builtin_id: &str,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(Vec::new())
    }
    async fn stats_summary(
        &self,
        _since: OffsetDateTime,
    ) -> Result<HashMap<ActionId, ActionStats>, StorageError> {
        Ok(HashMap::new())
    }

    async fn save_batch(
        &self,
        contexts: &[forge_types::ExecutionContext],
    ) -> Result<(), forge_storage::StorageError> {
        for ctx in contexts {
            forge_storage::HistoryRepo::save(self, ctx).await?;
        }
        Ok(())
    }

    async fn recent(
        &self,
        _limit: u32,
    ) -> Result<Vec<forge_types::ExecutionContext>, forge_storage::StorageError> {
        Ok(Vec::new())
    }
}

enum Behavior {
    Gate(watch::Receiver<bool>),
    UntilCancelled(Arc<Notify>),
    Instant,
    Panic,
    Fail,
}

struct ScriptedRunner {
    id: &'static str,
    behavior: Behavior,
}

#[async_trait]
impl SubActionRunner for ScriptedRunner {
    fn id(&self) -> &str {
        self.id
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        self.id
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id);
        match &self.behavior {
            Behavior::Gate(open) => {
                let mut open = open.clone();
                while !*open.borrow_and_update() {
                    if open.changed().await.is_err() {
                        break;
                    }
                }
            }
            Behavior::UntilCancelled(running) => {
                running.notify_one();
                ctx.cancel.cancelled().await;
                return (
                    timer.failed(forge_types::VerbatimText::new(CANCELLED_REASON).into()),
                    None,
                );
            }
            Behavior::Instant => {}
            Behavior::Panic => panic!("scripted runner panic"),
            Behavior::Fail => {
                return (
                    timer.failed(forge_types::VerbatimText::new(FAILURE_REASON).into()),
                    None,
                );
            }
        }
        (timer.success(), None)
    }
}

struct Rig {
    bus: Arc<EventBus>,
    engine: ActionEngineHandle,
    sched: QueueSchedulerHandle,
    slow_gate: watch::Sender<bool>,
    save_gate: watch::Sender<bool>,
    saved: mpsc::UnboundedReceiver<ExecutionContext>,
    repo: Arc<InMemoryActionRepo>,
    until_cancelled_running: Arc<Notify>,
}

impl Rig {
    fn open_slow_gate(&self) {
        self.slow_gate.send_replace(true);
    }

    async fn next_saved(&mut self) -> ExecutionContext {
        tokio::time::timeout(SAVE_DEADLINE, self.saved.recv())
            .await
            .expect("no run was saved to history in time")
            .expect("history channel closed")
    }
}

fn queue(id: QueueId, concurrency: u32) -> Queue {
    Queue {
        id,
        name: "q".to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::new(concurrency).unwrap(),
    }
}

fn step(kind_id: &str, config: SubActionConfig) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn action(queue_id: QueueId, steps: Vec<SubActionStep>) -> Action {
    common::sequential_action(ActionId::new(), "a", queue_id, steps)
}

fn single(queue_id: QueueId, kind_id: &str) -> Action {
    action(queue_id, vec![step(kind_id, SubActionConfig::new())])
}

fn long_wait_then_instant(queue_id: QueueId) -> Action {
    let mut wait = SubActionConfig::new();
    wait.insert(WAIT_MS_KEY.to_owned(), Variant::Int(LONG_WAIT_MS));
    action(
        queue_id,
        vec![
            step(WAIT_KIND_ID, wait),
            step(INSTANT_KIND_ID, SubActionConfig::new()),
        ],
    )
}

async fn rig(queues: Vec<Queue>, actions: &[&Action]) -> Rig {
    let repo = Arc::new(InMemoryActionRepo::new());
    for action in actions {
        repo.save(action).await.unwrap();
    }

    let until_cancelled_running = Arc::new(Notify::new());
    let (slow_gate, slow_open) = watch::channel(false);
    let mut registry = SubActionRegistry::new();
    for runner in [
        ScriptedRunner {
            id: SLOW_KIND_ID,
            behavior: Behavior::Gate(slow_open),
        },
        ScriptedRunner {
            id: UNTIL_CANCELLED_KIND_ID,
            behavior: Behavior::UntilCancelled(Arc::clone(&until_cancelled_running)),
        },
        ScriptedRunner {
            id: INSTANT_KIND_ID,
            behavior: Behavior::Instant,
        },
        ScriptedRunner {
            id: PANIC_KIND_ID,
            behavior: Behavior::Panic,
        },
        ScriptedRunner {
            id: FAILING_KIND_ID,
            behavior: Behavior::Fail,
        },
    ] {
        registry.register(Box::new(runner)).unwrap();
    }
    registry.register(Box::new(CoreLogicWaitRunner)).unwrap();

    let (saved_tx, saved) = mpsc::unbounded_channel();
    let (save_gate, save_open) = watch::channel(true);
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let repo_handle = Arc::clone(&repo);
    let repo: Arc<dyn ActionRepo> = repo;
    let mut instances = MockTriggerInstanceRepo::new();
    instances
        .expect_list_for_action()
        .returning(|_| Ok(Vec::new()));
    instances
        .expect_instances_by_action()
        .returning(|| Ok(Default::default()));
    let engine = spawn_action_engine(forge_runtime::ActionEngineParts {
        bus: Arc::clone(&bus),
        catalog: Catalog::new(
            Arc::clone(&repo),
            Arc::new(instances),
            CatalogRevision::new(),
        ),
        actions: repo,
        history: Arc::new(CapturingHistoryRepo {
            saved: saved_tx,
            gate: save_open,
        }),
        sub_action_registry: Arc::new(registry),
        cancel_registry: Arc::new(ActionCancelRegistry::new()),
        date_names: forge_runtime::DateNamesHandle::default(),
        config: forge_runtime::Config::default(),
    });
    let sched = QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), queues);
    Rig {
        bus,
        engine,
        sched,
        slow_gate,
        save_gate,
        saved,
        repo: repo_handle,
        until_cancelled_running,
    }
}

fn req(action: &Action) -> SchedulerRequest {
    SchedulerRequest {
        queue_id: action.queue_id,
        action_id: action.id,
        trigger_event_id: EventId::new(),
        trigger_kind: None,
        manual_test: false,
        initial_args: ArgStack::new(),
        admission: QueueAdmission::default(),
    }
}

fn action_id_of(ev: &Event) -> Option<String> {
    ev.payload
        .get("action_id")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

fn ids_of(events: &[Event], kind: &str) -> Vec<String> {
    events
        .iter()
        .filter(|ev| ev.kind == kind)
        .filter_map(action_id_of)
        .collect()
}

fn start_of(action: &Action) -> impl FnMut(&Event) -> bool {
    let target = action.id.to_string();
    move |ev| ev.kind == "action.start" && action_id_of(ev).as_deref() == Some(target.as_str())
}

fn done_of(action: &Action) -> impl FnMut(&Event) -> bool {
    let target = action.id.to_string();
    move |ev| ev.kind == "action.done" && action_id_of(ev).as_deref() == Some(target.as_str())
}

fn nth_start(n: usize) -> impl FnMut(&Event) -> bool {
    let mut seen = 0;
    move |ev| {
        if ev.kind == "action.start" {
            seen += 1;
        }
        seen == n
    }
}

async fn events_until(
    sub: &mut EventSubscription,
    mut stop: impl FnMut(&Event) -> bool,
) -> Vec<Event> {
    let mut seen: Vec<Event> = Vec::new();
    let mut quiet = 0;
    loop {
        match tokio::time::timeout(QUIET_STEP, sub.recv()).await {
            Ok(Ok(ev)) => {
                quiet = 0;
                let matched = stop(&ev);
                seen.push(ev);
                if matched {
                    return seen;
                }
            }
            Ok(Err(e)) => panic!("the bus closed before the awaited event: {e}"),
            Err(_) => {
                quiet += 1;
                assert!(
                    quiet < QUIET_ROUNDS,
                    "the bus went quiet before the awaited event, saw {:?}",
                    seen.iter().map(|ev| ev.kind.as_str()).collect::<Vec<_>>()
                );
                tokio::task::yield_now().await;
            }
        }
    }
}

fn drain(sub: &mut EventSubscription) -> Vec<Event> {
    let mut seen = Vec::new();
    while let Some(ev) = sub.try_recv().unwrap() {
        seen.push(ev);
    }
    seen
}

async fn quiesce() {
    tokio::time::sleep(QUIET_STEP).await;
}

async fn state_of(sched: &QueueSchedulerHandle, q_id: QueueId) -> QueueRuntimeState {
    sched
        .queue_states()
        .await
        .unwrap()
        .remove(&q_id)
        .expect("the queue stays registered")
}

async fn settle(
    sched: &QueueSchedulerHandle,
    q_id: QueueId,
    what: &str,
    want: impl Fn(&QueueRuntimeState) -> bool,
) {
    let mut last = None;
    for _ in 0..SETTLE_ROUNDS {
        let state = state_of(sched, q_id).await;
        if want(&state) {
            return;
        }
        last = Some(state);
        tokio::task::yield_now().await;
    }
    panic!("the queue never settled to {what}, last saw {last:?}");
}

#[tokio::test(start_paused = true)]
async fn a_long_execution_in_one_queue_does_not_delay_an_action_in_another() {
    let (qa, qb) = (QueueId::new(), QueueId::new());
    let slow = single(qa, SLOW_KIND_ID);
    let quick = single(qb, INSTANT_KIND_ID);
    let rig = rig(vec![queue(qa, 1), queue(qb, 1)], &[&slow, &quick]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(req(&slow)).await.unwrap();
    events_until(&mut sub, start_of(&slow)).await;
    rig.sched.dispatch(req(&quick)).await.unwrap();

    let seen = events_until(&mut sub, done_of(&quick)).await;
    assert!(
        !ids_of(&seen, "action.done").contains(&slow.id.to_string()),
        "the other queue's action must finish while the long one is still running"
    );
    rig.sched.shutdown();
}

async fn two_of_three_started_on_a_queue_of_two() -> (Rig, EventSubscription, QueueId, [Action; 3])
{
    let queue_id = QueueId::new();
    let actions = [
        single(queue_id, SLOW_KIND_ID),
        single(queue_id, SLOW_KIND_ID),
        single(queue_id, SLOW_KIND_ID),
    ];
    let rig = rig(
        vec![queue(queue_id, 2)],
        &[&actions[0], &actions[1], &actions[2]],
    )
    .await;
    let mut sub = rig.bus.subscribe();
    for action in &actions {
        rig.sched.dispatch(req(action)).await.unwrap();
    }
    let seen = events_until(&mut sub, nth_start(2)).await;
    assert_eq!(
        ids_of(&seen, "action.start"),
        vec![actions[0].id.to_string(), actions[1].id.to_string()],
        "a queue of two must start its first two tasks together"
    );
    (rig, sub, queue_id, actions)
}

#[tokio::test(start_paused = true)]
async fn a_queue_with_limit_two_runs_two_executions_at_once() {
    let (rig, _sub, queue_id, _) = two_of_three_started_on_a_queue_of_two().await;

    assert_eq!(
        state_of(&rig.sched, queue_id).await.in_flight,
        2,
        "both started executions must hold a slot while they run"
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_queue_with_limit_two_never_starts_a_third_while_two_run() {
    let (rig, mut sub, queue_id, _) = two_of_three_started_on_a_queue_of_two().await;

    quiesce().await;

    assert!(
        ids_of(&drain(&mut sub), "action.start").is_empty(),
        "no third execution may start while both slots are taken"
    );
    assert_eq!(state_of(&rig.sched, queue_id).await.pending, 1);
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_queue_slot_stays_taken_until_its_execution_ends() {
    let queue_id = QueueId::new();
    let slow = single(queue_id, SLOW_KIND_ID);
    let rig = rig(vec![queue(queue_id, 1)], &[&slow]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(req(&slow)).await.unwrap();
    events_until(&mut sub, start_of(&slow)).await;
    quiesce().await;
    assert_eq!(
        state_of(&rig.sched, queue_id).await.in_flight,
        1,
        "the slot must not be released while the execution is still running"
    );

    rig.open_slow_gate();
    settle(
        &rig.sched,
        queue_id,
        "the slot released after the run",
        |s| s.in_flight == 0,
    )
    .await;
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn shutdown_during_a_long_wait_records_the_run_as_cancelled_without_the_next_step() {
    let queue_id = QueueId::new();
    let waiting = long_wait_then_instant(queue_id);
    let mut rig = rig(vec![queue(queue_id, 1)], &[&waiting]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(req(&waiting)).await.unwrap();
    events_until(&mut sub, start_of(&waiting)).await;
    let stopped_at = tokio::time::Instant::now();
    rig.engine.clone().shutdown();

    let saved = rig.next_saved().await;
    assert_eq!(saved.outcome, ExecutionOutcome::Cancelled);
    assert_eq!(
        saved.telemetry.len(),
        1,
        "the step after the cancelled wait must never run"
    );
    assert!(
        stopped_at.elapsed() < Duration::from_secs(1),
        "the cancelled run must end long before its 60 s wait, took {:?}",
        stopped_at.elapsed()
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn shutdown_releases_the_queue_slot_of_the_cancelled_execution() {
    let queue_id = QueueId::new();
    let waiting = long_wait_then_instant(queue_id);
    let mut rig = rig(vec![queue(queue_id, 1)], &[&waiting]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(req(&waiting)).await.unwrap();
    events_until(&mut sub, start_of(&waiting)).await;
    rig.engine.clone().shutdown();
    rig.next_saved().await;

    settle(
        &rig.sched,
        queue_id,
        "the cancelled run's slot released",
        |s| s.in_flight == 0,
    )
    .await;
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_panicking_runner_releases_its_slot_and_the_next_job_still_runs() {
    let queue_id = QueueId::new();
    let panics = single(queue_id, PANIC_KIND_ID);
    let next = single(queue_id, INSTANT_KIND_ID);
    let rig = rig(vec![queue(queue_id, 1)], &[&panics, &next]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(req(&panics)).await.unwrap();
    rig.sched.dispatch(req(&next)).await.unwrap();

    let seen = events_until(&mut sub, done_of(&next)).await;
    assert!(
        ids_of(&seen, "action.start").contains(&panics.id.to_string()),
        "the panicking action must have started before the next one ran"
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_long_quick_action_does_not_block_the_next_quick_action() {
    let mut rig = rig(Vec::new(), &[]).await;

    rig.engine
        .execute_quick_action(
            step(SLOW_KIND_ID, SubActionConfig::new()),
            forge_runtime::QuickActionOrigin {
                builtin_id: "obs".to_owned(),
                label: "slow".to_owned(),
            },
            None,
        )
        .await
        .unwrap();
    rig.engine
        .execute_quick_action(
            step(INSTANT_KIND_ID, SubActionConfig::new()),
            forge_runtime::QuickActionOrigin {
                builtin_id: "obs".to_owned(),
                label: "instant".to_owned(),
            },
            None,
        )
        .await
        .unwrap();

    let saved = rig.next_saved().await;
    assert!(
        matches!(&saved.metadata, ExecutionMetadata::QuickAction { label, .. } if label == "instant"),
        "the instant quick action must finish while the slow one runs, got {:?}",
        saved.metadata
    );
}

#[tokio::test(start_paused = true)]
async fn a_run_that_finishes_last_keeps_the_earlier_start_time() {
    let queue_id = QueueId::new();
    let first = single(queue_id, SLOW_KIND_ID);
    let second = single(queue_id, INSTANT_KIND_ID);
    let mut rig = rig(vec![queue(queue_id, 2)], &[&first, &second]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(req(&first)).await.unwrap();
    events_until(&mut sub, start_of(&first)).await;
    rig.sched.dispatch(req(&second)).await.unwrap();
    let finished_first = rig.next_saved().await;
    rig.open_slow_gate();
    let finished_last = rig.next_saved().await;

    assert_eq!(
        (finished_first.action_id, finished_last.action_id),
        (second.id, first.id),
        "the later-started run must be saved first for this scenario to hold"
    );
    assert!(
        finished_last.started_at < finished_first.started_at,
        "a run's start time must be taken when it starts, not when it is saved"
    );
    rig.sched.shutdown();
}

type OutcomeCheck = fn(&SubActionOutcome) -> bool;

async fn quick(engine: &ActionEngineHandle, kind_id: &str) -> PendingQuickAction {
    engine
        .execute_quick_action(
            step(kind_id, SubActionConfig::new()),
            forge_runtime::QuickActionOrigin {
                builtin_id: "obs".to_owned(),
                label: kind_id.to_owned(),
            },
            None,
        )
        .await
        .expect("the engine accepts the quick action")
}

#[tokio::test(start_paused = true)]
async fn a_quick_action_reports_how_its_step_ended() {
    let rig = rig(Vec::new(), &[]).await;

    let cases: [(&str, OutcomeCheck); 3] = [
        (INSTANT_KIND_ID, |o| matches!(o, SubActionOutcome::Success)),
        (
            FAILING_KIND_ID,
            |o| matches!(o, SubActionOutcome::Failed(reason) if reason.code_or_text() == FAILURE_REASON),
        ),
        (UNREGISTERED_KIND_ID, |o| {
            matches!(o, SubActionOutcome::Skipped(_))
        }),
    ];
    for (kind_id, expected) in cases {
        let outcome = quick(&rig.engine, kind_id)
            .await
            .outcome()
            .await
            .expect("a running engine reports every quick action's outcome");
        assert!(expected(&outcome), "{kind_id} reported {outcome:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_quick_action_still_in_the_intake_at_shutdown_reports_no_outcome() {
    let rig = rig(Vec::new(), &[]).await;

    rig.engine.clone().shutdown();
    let pending = quick(&rig.engine, INSTANT_KIND_ID).await;

    assert!(matches!(
        pending.outcome().await,
        Err(RuntimeError::EngineNoOutcome)
    ));
}

#[tokio::test(start_paused = true)]
async fn shutdown_cancels_a_running_quick_action() {
    let rig = rig(Vec::new(), &[]).await;
    let pending = quick(&rig.engine, UNTIL_CANCELLED_KIND_ID).await;
    rig.until_cancelled_running.notified().await;

    rig.engine.clone().shutdown();

    let outcome = tokio::time::timeout(QUIET_STEP, pending.outcome())
        .await
        .expect("the running quick action never saw the shutdown")
        .expect("a step that ran reports its outcome");
    assert!(
        matches!(&outcome, SubActionOutcome::Failed(reason) if reason.code_or_text() == CANCELLED_REASON),
        "{outcome:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_quick_action_whose_outcome_nobody_awaits_still_runs_and_is_saved() {
    let mut rig = rig(Vec::new(), &[]).await;

    drop(quick(&rig.engine, INSTANT_KIND_ID).await);

    let saved = rig.next_saved().await;
    assert!(
        matches!(&saved.metadata, ExecutionMetadata::QuickAction { label, .. } if label == INSTANT_KIND_ID),
        "the abandoned quick action must still record its run, got {:?}",
        saved.metadata
    );
}

#[tokio::test(start_paused = true)]
async fn a_quick_action_reports_its_outcome_without_waiting_for_its_run_to_be_saved() {
    let mut rig = rig(Vec::new(), &[]).await;
    rig.save_gate.send_replace(false);

    let outcome = tokio::time::timeout(
        QUIET_STEP,
        quick(&rig.engine, INSTANT_KIND_ID).await.outcome(),
    )
    .await;
    assert!(
        matches!(outcome, Ok(Ok(SubActionOutcome::Success))),
        "a slow history write must not hold back the caller's outcome"
    );

    rig.save_gate.send_replace(true);
    let saved = tokio::time::timeout(Duration::from_secs(5), rig.saved.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        saved.metadata,
        ExecutionMetadata::QuickAction { .. }
    ));
}

fn reason_of(ev: &Event) -> Option<&str> {
    ev.payload.get("reason").and_then(|v| v.as_str())
}

fn queue_of(ev: &Event) -> Option<&str> {
    ev.payload.get("queue_id").and_then(|v| v.as_str())
}

async fn refused_run(
    rig: &mut Rig,
    action: &Action,
    sub: &mut EventSubscription,
) -> (Event, ExecutionContext) {
    rig.sched.dispatch(req(action)).await.unwrap();
    let skipped = events_until(sub, |ev| ev.kind == "action.skipped")
        .await
        .pop()
        .unwrap();
    (skipped, rig.next_saved().await)
}

fn failure_code(saved: &ExecutionContext) -> String {
    match &saved.outcome {
        ExecutionOutcome::Failed(reason) => reason.code_or_text(),
        other => panic!("a refused run is saved as failed, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_run_of_a_disabled_action_is_skipped_and_saved_as_failed_on_its_own_queue() {
    let queue_id = QueueId::new();
    let disabled = Action {
        enabled: false,
        ..single(queue_id, INSTANT_KIND_ID)
    };
    let mut rig = rig(vec![queue(queue_id, 1)], &[&disabled]).await;
    let mut sub = rig.bus.subscribe();

    let (skipped, saved) = refused_run(&mut rig, &disabled, &mut sub).await;

    assert_eq!(reason_of(&skipped), Some(RunRefusal::ActionDisabled.code()));
    assert_eq!(queue_of(&skipped), Some(queue_id.to_string().as_str()));
    assert_eq!(failure_code(&saved), "core.action_disabled");
    assert!(saved.telemetry.is_empty(), "no step of a refused run runs");
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_run_of_a_deleted_action_is_skipped_and_saved_as_failed() {
    let queue_id = QueueId::new();
    let deleted = single(queue_id, INSTANT_KIND_ID);
    let mut rig = rig(vec![queue(queue_id, 1)], &[]).await;
    let mut sub = rig.bus.subscribe();

    let (skipped, saved) = refused_run(&mut rig, &deleted, &mut sub).await;

    assert_eq!(reason_of(&skipped), Some(RunRefusal::ActionNotFound.code()));
    assert_eq!(
        queue_of(&skipped),
        Some(DEFAULT_QUEUE_ID.to_string().as_str())
    );
    assert_eq!(failure_code(&saved), "core.unknown_action");
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_run_is_skipped_and_saved_as_failed_when_actions_cannot_be_read() {
    let queue_id = QueueId::new();
    let wanted = single(queue_id, INSTANT_KIND_ID);
    let mut rig = rig(vec![queue(queue_id, 1)], &[&wanted]).await;
    rig.repo.set_list_unreadable(true);
    let mut sub = rig.bus.subscribe();

    let (skipped, saved) = refused_run(&mut rig, &wanted, &mut sub).await;

    assert_eq!(
        reason_of(&skipped),
        Some(RunRefusal::ActionsUnavailable.code())
    );
    assert_eq!(failure_code(&saved), "core.storage_failed");
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_lookup_failure_for_an_action_outside_the_catalog_is_saved_as_a_storage_failure() {
    let queue_id = QueueId::new();
    let disabled = Action {
        enabled: false,
        ..single(queue_id, INSTANT_KIND_ID)
    };
    let mut rig = rig(vec![queue(queue_id, 1)], &[&disabled]).await;
    rig.repo.set_get_unreadable(true);
    let mut sub = rig.bus.subscribe();

    let (skipped, saved) = refused_run(&mut rig, &disabled, &mut sub).await;

    assert_eq!(
        reason_of(&skipped),
        Some(RunRefusal::ActionsUnavailable.code())
    );
    assert_eq!(failure_code(&saved), "core.storage_failed");
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_queued_run_that_cannot_reach_a_stopped_engine_is_skipped_and_frees_its_slot() {
    let queue_id = QueueId::new();
    let action = single(queue_id, INSTANT_KIND_ID);
    let rig = rig(vec![queue(queue_id, 1)], &[&action]).await;
    let mut sub = rig.bus.subscribe();
    rig.engine.clone().shutdown();
    while rig
        .engine
        .dispatch(ExecutionRequest {
            action_id: action.id,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            initial_args: ArgStack::new(),
        })
        .await
        .is_ok()
    {
        tokio::task::yield_now().await;
    }

    rig.sched.dispatch(req(&action)).await.unwrap();

    let skipped = events_until(&mut sub, |ev| ev.kind == "action.skipped")
        .await
        .pop()
        .unwrap();
    assert_eq!(reason_of(&skipped), Some(RunRefusal::EngineStopped.code()));
    settle(
        &rig.sched,
        queue_id,
        "the undelivered run's slot released",
        |s| s.in_flight == 0,
    )
    .await;
    rig.sched.shutdown();
}

fn concurrent(action: Action) -> Action {
    Action {
        concurrent: true,
        ..action
    }
}

fn admitted(action: &Action) -> SchedulerRequest {
    SchedulerRequest {
        admission: QueueAdmission::from(action),
        ..req(action)
    }
}

fn step_trace(events: &[Event]) -> Vec<(&str, i64)> {
    events
        .iter()
        .filter(|ev| ev.kind == "subaction.run" || ev.kind == "subaction.done")
        .filter_map(|ev| Some((ev.kind.as_str(), ev.payload.get("step_index")?.as_i64()?)))
        .collect()
}

#[tokio::test(start_paused = true)]
async fn an_alongside_run_starts_while_the_only_slot_is_busy() {
    let queue_id = QueueId::new();
    let slow = single(queue_id, SLOW_KIND_ID);
    let beside = concurrent(single(queue_id, INSTANT_KIND_ID));
    let rig = rig(vec![queue(queue_id, 1)], &[&slow, &beside]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(req(&slow)).await.unwrap();
    events_until(&mut sub, start_of(&slow)).await;
    rig.sched.dispatch(admitted(&beside)).await.unwrap();

    let seen = events_until(&mut sub, done_of(&beside)).await;
    assert!(
        !ids_of(&seen, "action.done").contains(&slow.id.to_string()),
        "the concurrent action must finish while the slow one still holds the only slot"
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn alongside_runs_count_as_in_flight_beyond_the_queue_limit() {
    let queue_id = QueueId::new();
    let slow = single(queue_id, SLOW_KIND_ID);
    let beside = concurrent(single(queue_id, SLOW_KIND_ID));
    let rig = rig(vec![queue(queue_id, 1)], &[&slow, &beside]).await;

    rig.sched.dispatch(req(&slow)).await.unwrap();
    rig.sched.dispatch(admitted(&beside)).await.unwrap();

    settle(
        &rig.sched,
        queue_id,
        "both runs in flight on a queue of one",
        |s| s.in_flight == 2 && s.pending == 0,
    )
    .await;
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_running_alongside_run_leaves_the_slot_to_a_gated_run() {
    let queue_id = QueueId::new();
    let beside = concurrent(single(queue_id, SLOW_KIND_ID));
    let gated = single(queue_id, INSTANT_KIND_ID);
    let rig = rig(vec![queue(queue_id, 1)], &[&beside, &gated]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(admitted(&beside)).await.unwrap();
    events_until(&mut sub, start_of(&beside)).await;
    rig.sched.dispatch(req(&gated)).await.unwrap();

    let seen = events_until(&mut sub, done_of(&gated)).await;
    assert!(
        !ids_of(&seen, "action.done").contains(&beside.id.to_string()),
        "the gated action must take the slot while the concurrent one still runs"
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_concurrent_action_runs_its_steps_one_after_another() {
    let queue_id = QueueId::new();
    let both = concurrent(action(
        queue_id,
        vec![
            step(SLOW_KIND_ID, SubActionConfig::new()),
            step(INSTANT_KIND_ID, SubActionConfig::new()),
        ],
    ));
    let rig = rig(vec![queue(queue_id, 1)], &[&both]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(admitted(&both)).await.unwrap();
    let mut seen = events_until(&mut sub, start_of(&both)).await;
    quiesce().await;
    seen.extend(drain(&mut sub));
    rig.open_slow_gate();
    seen.extend(events_until(&mut sub, done_of(&both)).await);

    assert_eq!(
        step_trace(&seen),
        vec![
            ("subaction.run", 0),
            ("subaction.done", 0),
            ("subaction.run", 1),
            ("subaction.done", 1),
        ]
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn holding_withholds_an_alongside_run_until_the_queue_resumes() {
    let queue_id = QueueId::new();
    let beside = concurrent(single(queue_id, INSTANT_KIND_ID));
    let rig = rig(vec![queue(queue_id, 1)], &[&beside]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched
        .set_mode(queue_id, QueueMode::HOLDING)
        .await
        .unwrap();
    rig.sched.dispatch(admitted(&beside)).await.unwrap();
    settle(&rig.sched, queue_id, "the run held", |s| s.pending == 1).await;
    quiesce().await;
    let while_held = drain(&mut sub);
    rig.sched
        .set_mode(queue_id, QueueMode::RUNNING)
        .await
        .unwrap();
    let after_resume = events_until(&mut sub, done_of(&beside)).await;

    assert_eq!(
        (
            ids_of(&while_held, "action.start"),
            ids_of(&after_resume, "action.start")
        ),
        (Vec::new(), vec![beside.id.to_string()])
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_bypass_pause_alongside_run_passes_a_held_queue_while_a_plain_one_waits() {
    let queue_id = QueueId::new();
    let plain = concurrent(single(queue_id, INSTANT_KIND_ID));
    let bypass = Action {
        bypass_pause: true,
        ..concurrent(single(queue_id, INSTANT_KIND_ID))
    };
    let rig = rig(vec![queue(queue_id, 1)], &[&plain, &bypass]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched
        .set_mode(queue_id, QueueMode::HOLDING)
        .await
        .unwrap();
    rig.sched.dispatch(admitted(&plain)).await.unwrap();
    rig.sched.dispatch(admitted(&bypass)).await.unwrap();
    let seen = events_until(&mut sub, done_of(&bypass)).await;

    assert_eq!(
        (
            ids_of(&seen, "action.start"),
            state_of(&rig.sched, queue_id).await.pending
        ),
        (vec![bypass.id.to_string()], 1)
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn paused_and_draining_queues_refuse_an_alongside_run() {
    let queue_id = QueueId::new();
    let beside = concurrent(single(queue_id, INSTANT_KIND_ID));
    let rig = rig(vec![queue(queue_id, 1)], &[&beside]).await;
    let mut sub = rig.bus.subscribe();

    for (mode, reason) in [
        (QueueMode::PAUSED, RunRefusal::QueuePaused.code()),
        (QueueMode::DRAINING, RunRefusal::QueueDraining.code()),
    ] {
        rig.sched.set_mode(queue_id, mode).await.unwrap();
        rig.sched.dispatch(admitted(&beside)).await.unwrap();
        let seen = events_until(&mut sub, |ev| ev.kind == "action.skipped").await;
        assert_eq!(
            seen.last()
                .and_then(|ev| ev.payload.get("reason"))
                .and_then(|v| v.as_str()),
            Some(reason),
            "{mode:?}"
        );
    }
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn an_alongside_run_past_the_pending_cap_is_refused_as_overflow() {
    let queue_id = QueueId::new();
    let plain = single(queue_id, INSTANT_KIND_ID);
    let beside = concurrent(single(queue_id, INSTANT_KIND_ID));
    let rig = rig(vec![queue(queue_id, 1)], &[&plain, &beside]).await;

    rig.sched
        .set_mode(queue_id, QueueMode::HOLDING)
        .await
        .unwrap();
    for _ in 0..MAX_PENDING_PER_QUEUE {
        rig.sched.dispatch(req(&plain)).await.unwrap();
    }
    rig.sched.dispatch(admitted(&beside)).await.unwrap();

    settle(
        &rig.sched,
        queue_id,
        "the alongside run counted as overflow",
        |s| s.pending == MAX_PENDING_PER_QUEUE && s.overflowed == 1,
    )
    .await;
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn clearing_discards_withheld_alongside_runs() {
    let queue_id = QueueId::new();
    let beside = concurrent(single(queue_id, INSTANT_KIND_ID));
    let rig = rig(vec![queue(queue_id, 1)], &[&beside]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched
        .set_mode(queue_id, QueueMode::HOLDING)
        .await
        .unwrap();
    for _ in 0..2 {
        rig.sched.dispatch(admitted(&beside)).await.unwrap();
    }
    settle(&rig.sched, queue_id, "two runs held", |s| s.pending == 2).await;
    rig.sched
        .clear(queue_id, forge_runtime::ClearMode::KeepCurrent)
        .await
        .unwrap();
    rig.sched
        .set_mode(queue_id, QueueMode::RUNNING)
        .await
        .unwrap();
    quiesce().await;

    assert!(
        ids_of(&drain(&mut sub), "action.start").is_empty(),
        "cleared alongside runs must never start"
    );
    rig.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn clearing_without_keeping_current_cancels_a_running_alongside_run() {
    let queue_id = QueueId::new();
    let waiting = concurrent(long_wait_then_instant(queue_id));
    let mut rig = rig(vec![queue(queue_id, 1)], &[&waiting]).await;
    let mut sub = rig.bus.subscribe();

    rig.sched.dispatch(admitted(&waiting)).await.unwrap();
    events_until(&mut sub, start_of(&waiting)).await;
    rig.sched
        .clear(queue_id, forge_runtime::ClearMode::CancelCurrent)
        .await
        .unwrap();

    assert_eq!(rig.next_saved().await.outcome, ExecutionOutcome::Cancelled);
    rig.sched.shutdown();
}

const STOP_GRACE: Duration = Duration::from_secs(1);

#[tokio::test(start_paused = true)]
async fn stop_returns_only_after_the_running_action_and_its_queue_tasks_have_ended() {
    let queue_id = QueueId::new();
    let waiting = long_wait_then_instant(queue_id);
    let mut rig = rig(vec![queue(queue_id, 1)], &[&waiting]).await;
    let mut sub = rig.bus.subscribe();
    rig.sched.dispatch(req(&waiting)).await.unwrap();
    events_until(&mut sub, start_of(&waiting)).await;

    rig.sched.shutdown();
    rig.engine.shutdown();
    let (queues, engine) = tokio::join!(
        rig.sched.stopped(STOP_GRACE),
        rig.engine.stopped(STOP_GRACE)
    );

    assert_eq!(
        (queues, engine),
        (TaskShutdown::Drained, TaskShutdown::Drained)
    );
    assert_eq!(rig.next_saved().await.outcome, ExecutionOutcome::Cancelled);
}

#[tokio::test(start_paused = true)]
async fn stop_with_nothing_running_ends_the_engine_and_every_queue_worker() {
    let queue_id = QueueId::new();
    let rig = rig(vec![queue(queue_id, 1), queue(QueueId::new(), 2)], &[]).await;

    let (queues, engine) = tokio::join!(
        rig.sched.stopped(STOP_GRACE),
        rig.engine.stopped(STOP_GRACE)
    );

    assert_eq!(
        (queues, engine),
        (TaskShutdown::Drained, TaskShutdown::Drained)
    );
}

#[tokio::test(start_paused = true)]
async fn stop_ends_a_running_quick_action_before_returning() {
    let rig = rig(Vec::new(), &[]).await;
    let pending = quick(&rig.engine, UNTIL_CANCELLED_KIND_ID).await;
    rig.until_cancelled_running.notified().await;

    let stopped = rig.engine.stopped(STOP_GRACE).await;

    assert_eq!(stopped, TaskShutdown::Drained);
    assert!(pending.outcome().await.is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_run_that_ignores_the_cancel_is_dropped_when_the_grace_runs_out() {
    let queue_id = QueueId::new();
    let stuck = single(queue_id, SLOW_KIND_ID);
    let rig = rig(vec![queue(queue_id, 1)], &[&stuck]).await;
    let mut sub = rig.bus.subscribe();
    rig.sched.dispatch(req(&stuck)).await.unwrap();
    events_until(&mut sub, start_of(&stuck)).await;
    let asked = tokio::time::Instant::now();

    let (queues, engine) = tokio::join!(
        rig.sched.stopped(STOP_GRACE),
        rig.engine.stopped(STOP_GRACE)
    );

    assert_eq!(asked.elapsed(), STOP_GRACE);
    assert!(matches!(queues, TaskShutdown::Aborted { .. }), "{queues:?}");
    assert!(matches!(engine, TaskShutdown::Aborted { .. }), "{engine:?}");
}
