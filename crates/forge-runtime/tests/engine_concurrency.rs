#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_events::Event;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
    SubActionRunner,
};
use forge_runtime::sub_action_runners::{CoreLogicWaitRunner, WAIT_KIND_ID, WAIT_MS_KEY};
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, Catalog, DispatchError, EventBus, EventSubscription,
    NullEventLogRepo, PendingQuickAction, QueueRuntimeState, QueueScheduler, QueueSchedulerHandle,
    SchedulerRequest, spawn_action_engine,
};
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{
    ActionRepo, ActionStats, ActionTelemetry, CatalogRevision, ExecutionStatus, HistoryRepo,
    StorageError,
};
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionContext, ExecutionMetadata, ExecutionMode,
    ExecutionOutcome, Queue, QueueId, SubActionConfig, SubActionOutcome, SubActionStep,
    SubActionTelemetry, Variant,
};
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};

const SLOW_KIND_ID: &str = "test.slow";
const INSTANT_KIND_ID: &str = "test.instant";
const PANIC_KIND_ID: &str = "test.panic";
const FAILING_KIND_ID: &str = "test.fail";
const UNREGISTERED_KIND_ID: &str = "test.unregistered";
const FAILURE_REASON: &str = "scene not found";
const LONG_WAIT_MS: i64 = 60_000;
const SETTLE_ROUNDS: usize = 200;
const QUIET_STEP: Duration = Duration::from_secs(5);
const QUIET_ROUNDS: usize = 4;
const SAVE_DEADLINE: Duration = Duration::from_secs(2);

/// Keeps the whole fixture inside the process: a sqlite-backed repo waits on a pool
/// timeout, which the paused clock fires the instant the runtime looks idle.
#[derive(Default)]
struct MemoryActionRepo {
    actions: Mutex<HashMap<ActionId, Action>>,
}

#[async_trait]
impl ActionRepo for MemoryActionRepo {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(self.actions.lock().unwrap().values().cloned().collect())
    }
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(self.actions.lock().unwrap().get(&id).cloned())
    }
    async fn save(&self, action: &Action) -> Result<(), StorageError> {
        self.actions
            .lock()
            .unwrap()
            .insert(action.id, action.clone());
        Ok(())
    }
    async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        Ok(self.actions.lock().unwrap().remove(&id).is_some())
    }
    async fn list_by_group<'a>(
        &'a self,
        _group: Option<&'a str>,
    ) -> Result<Vec<Action>, StorageError> {
        Ok(Vec::new())
    }
    async fn telemetry(&self, _id: ActionId) -> Result<ActionTelemetry, StorageError> {
        Ok(ActionTelemetry::default())
    }
    async fn record_execution(
        &self,
        _action_id: ActionId,
        _started_at: OffsetDateTime,
        _duration_ms: u64,
        _status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn prune_executions_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

/// Hands every saved run to the test in the order the engine saved it, each save held
/// until the test's save gate is open.
struct CapturingHistoryRepo {
    saved: mpsc::UnboundedSender<ExecutionContext>,
    gate: watch::Receiver<bool>,
}

#[async_trait]
impl HistoryRepo for CapturingHistoryRepo {
    async fn save(&self, ctx: &ExecutionContext) -> Result<(), StorageError> {
        let mut gate = self.gate.clone();
        let _ = gate.wait_for(|open| *open).await;
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
    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

enum Behavior {
    /// Blocks until the test opens the gate; deliberately blind to cancellation.
    Gate(watch::Receiver<bool>),
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
            Behavior::Instant => {}
            Behavior::Panic => panic!("scripted runner panic"),
            Behavior::Fail => return (timer.failed(FAILURE_REASON), None),
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
        concurrency,
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
    Action {
        id: ActionId::new(),
        name: "a".to_owned(),
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: steps,
    }
}

fn single(queue_id: QueueId, kind_id: &str) -> Action {
    action(queue_id, vec![step(kind_id, SubActionConfig::new())])
}

/// A 60 s wait followed by a step that must never run once the wait is cancelled.
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
    let repo = MemoryActionRepo::default();
    for a in actions {
        repo.save(a).await.unwrap();
    }

    let (slow_gate, slow_open) = watch::channel(false);
    let mut registry = SubActionRegistry::new();
    for runner in [
        ScriptedRunner {
            id: SLOW_KIND_ID,
            behavior: Behavior::Gate(slow_open),
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
    let repo: Arc<dyn ActionRepo> = Arc::new(repo);
    let mut instances = MockTriggerInstanceRepo::new();
    instances
        .expect_list_for_action()
        .returning(|_| Ok(Vec::new()));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Catalog::new(
            Arc::clone(&repo),
            Arc::new(instances),
            CatalogRevision::new(),
        ),
        repo,
        Arc::new(CapturingHistoryRepo {
            saved: saved_tx,
            gate: save_open,
        }),
        Arc::new(registry),
        Arc::new(ActionCancelRegistry::new()),
    );
    let sched = QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), queues);
    Rig {
        bus,
        engine,
        sched,
        slow_gate,
        save_gate,
        saved,
    }
}

fn req(a: &Action) -> SchedulerRequest {
    SchedulerRequest {
        queue_id: a.queue_id,
        action_id: a.id,
        trigger_event_id: EventId::new(),
        trigger_kind: None,
        initial_args: ArgStack::new(),
        bypass_pause: false,
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

fn start_of(a: &Action) -> impl FnMut(&Event) -> bool {
    let target = a.id.to_string();
    move |ev| ev.kind == "action.start" && action_id_of(ev).as_deref() == Some(target.as_str())
}

fn done_of(a: &Action) -> impl FnMut(&Event) -> bool {
    let target = a.id.to_string();
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

/// Reads bus events until `stop` matches, returning everything seen including it.
///
/// Why: every party is a task on the paused runtime, so an elapsed step means the system is
/// quiescent and the awaited event is never coming; a serialized engine fails here at once
/// instead of hanging.
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

/// Why: the paused clock only advances once every task is idle, so this returns when the
/// engine and scheduler have done everything they are going to do without a new input.
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
    let r = rig(vec![queue(qa, 1), queue(qb, 1)], &[&slow, &quick]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(req(&slow)).await.unwrap();
    events_until(&mut sub, start_of(&slow)).await;
    r.sched.dispatch(req(&quick)).await.unwrap();

    let seen = events_until(&mut sub, done_of(&quick)).await;
    assert!(
        !ids_of(&seen, "action.done").contains(&slow.id.to_string()),
        "the other queue's action must finish while the long one is still running"
    );
    r.sched.shutdown();
}

/// Dispatches three blocked actions to a queue of two and waits for two of them to start.
async fn two_of_three_started_on_a_queue_of_two() -> (Rig, EventSubscription, QueueId, [Action; 3])
{
    let q = QueueId::new();
    let actions = [
        single(q, SLOW_KIND_ID),
        single(q, SLOW_KIND_ID),
        single(q, SLOW_KIND_ID),
    ];
    let r = rig(vec![queue(q, 2)], &[&actions[0], &actions[1], &actions[2]]).await;
    let mut sub = r.bus.subscribe();
    for a in &actions {
        r.sched.dispatch(req(a)).await.unwrap();
    }
    let seen = events_until(&mut sub, nth_start(2)).await;
    assert_eq!(
        ids_of(&seen, "action.start"),
        vec![actions[0].id.to_string(), actions[1].id.to_string()],
        "a queue of two must start its first two tasks together"
    );
    (r, sub, q, actions)
}

#[tokio::test(start_paused = true)]
async fn a_queue_with_limit_two_runs_two_executions_at_once() {
    let (r, _sub, q, _) = two_of_three_started_on_a_queue_of_two().await;

    assert_eq!(
        state_of(&r.sched, q).await.in_flight,
        2,
        "both started executions must hold a slot while they run"
    );
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_queue_with_limit_two_never_starts_a_third_while_two_run() {
    let (r, mut sub, q, _) = two_of_three_started_on_a_queue_of_two().await;

    quiesce().await;

    assert!(
        ids_of(&drain(&mut sub), "action.start").is_empty(),
        "no third execution may start while both slots are taken"
    );
    assert_eq!(state_of(&r.sched, q).await.pending, 1);
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_queue_slot_stays_taken_until_its_execution_ends() {
    let q = QueueId::new();
    let slow = single(q, SLOW_KIND_ID);
    let r = rig(vec![queue(q, 1)], &[&slow]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(req(&slow)).await.unwrap();
    events_until(&mut sub, start_of(&slow)).await;
    quiesce().await;
    assert_eq!(
        state_of(&r.sched, q).await.in_flight,
        1,
        "the slot must not be released while the execution is still running"
    );

    r.open_slow_gate();
    settle(&r.sched, q, "the slot released after the run", |s| {
        s.in_flight == 0
    })
    .await;
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn shutdown_during_a_long_wait_records_the_run_as_cancelled_without_the_next_step() {
    let q = QueueId::new();
    let waiting = long_wait_then_instant(q);
    let mut r = rig(vec![queue(q, 1)], &[&waiting]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(req(&waiting)).await.unwrap();
    events_until(&mut sub, start_of(&waiting)).await;
    let stopped_at = tokio::time::Instant::now();
    r.engine.clone().shutdown();

    let saved = r.next_saved().await;
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
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn shutdown_releases_the_queue_slot_of_the_cancelled_execution() {
    let q = QueueId::new();
    let waiting = long_wait_then_instant(q);
    let mut r = rig(vec![queue(q, 1)], &[&waiting]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(req(&waiting)).await.unwrap();
    events_until(&mut sub, start_of(&waiting)).await;
    r.engine.clone().shutdown();
    r.next_saved().await;

    settle(&r.sched, q, "the cancelled run's slot released", |s| {
        s.in_flight == 0
    })
    .await;
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_panicking_runner_releases_its_slot_and_the_next_job_still_runs() {
    let q = QueueId::new();
    let panics = single(q, PANIC_KIND_ID);
    let next = single(q, INSTANT_KIND_ID);
    let r = rig(vec![queue(q, 1)], &[&panics, &next]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(req(&panics)).await.unwrap();
    r.sched.dispatch(req(&next)).await.unwrap();

    let seen = events_until(&mut sub, done_of(&next)).await;
    assert!(
        ids_of(&seen, "action.start").contains(&panics.id.to_string()),
        "the panicking action must have started before the next one ran"
    );
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_long_quick_action_does_not_block_the_next_quick_action() {
    let mut r = rig(Vec::new(), &[]).await;

    r.engine
        .execute_quick_action(
            step(SLOW_KIND_ID, SubActionConfig::new()),
            "obs".to_owned(),
            "slow".to_owned(),
            None,
        )
        .await
        .unwrap();
    r.engine
        .execute_quick_action(
            step(INSTANT_KIND_ID, SubActionConfig::new()),
            "obs".to_owned(),
            "instant".to_owned(),
            None,
        )
        .await
        .unwrap();

    let saved = r.next_saved().await;
    assert!(
        matches!(&saved.metadata, ExecutionMetadata::QuickAction { label, .. } if label == "instant"),
        "the instant quick action must finish while the slow one runs, got {:?}",
        saved.metadata
    );
}

#[tokio::test(start_paused = true)]
async fn a_run_that_finishes_last_keeps_the_earlier_start_time() {
    let q = QueueId::new();
    let first = single(q, SLOW_KIND_ID);
    let second = single(q, INSTANT_KIND_ID);
    let mut r = rig(vec![queue(q, 2)], &[&first, &second]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(req(&first)).await.unwrap();
    events_until(&mut sub, start_of(&first)).await;
    r.sched.dispatch(req(&second)).await.unwrap();
    let finished_first = r.next_saved().await;
    r.open_slow_gate();
    let finished_last = r.next_saved().await;

    assert_eq!(
        (finished_first.action_id, finished_last.action_id),
        (second.id, first.id),
        "the later-started run must be saved first for this scenario to hold"
    );
    assert!(
        finished_last.started_at < finished_first.started_at,
        "a run's start time must be taken when it starts, not when it is saved"
    );
    r.sched.shutdown();
}

type OutcomeCheck = fn(&SubActionOutcome) -> bool;

async fn quick(engine: &ActionEngineHandle, kind_id: &str) -> PendingQuickAction {
    engine
        .execute_quick_action(
            step(kind_id, SubActionConfig::new()),
            "obs".to_owned(),
            kind_id.to_owned(),
            None,
        )
        .await
        .expect("the engine accepts the quick action")
}

#[tokio::test(start_paused = true)]
async fn a_quick_action_reports_how_its_step_ended() {
    let r = rig(Vec::new(), &[]).await;

    let cases: [(&str, OutcomeCheck); 3] = [
        (INSTANT_KIND_ID, |o| matches!(o, SubActionOutcome::Success)),
        (
            FAILING_KIND_ID,
            |o| matches!(o, SubActionOutcome::Failed(reason) if reason == FAILURE_REASON),
        ),
        (UNREGISTERED_KIND_ID, |o| {
            matches!(o, SubActionOutcome::Skipped(_))
        }),
    ];
    for (kind_id, expected) in cases {
        let outcome = quick(&r.engine, kind_id)
            .await
            .outcome()
            .await
            .expect("a running engine reports every quick action's outcome");
        assert!(expected(&outcome), "{kind_id} reported {outcome:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn a_quick_action_still_in_the_intake_at_shutdown_reports_no_outcome() {
    let r = rig(Vec::new(), &[]).await;

    r.engine.clone().shutdown();
    let pending = quick(&r.engine, INSTANT_KIND_ID).await;

    assert!(matches!(
        pending.outcome().await,
        Err(DispatchError::NoOutcome)
    ));
}

#[tokio::test(start_paused = true)]
async fn a_quick_action_whose_outcome_nobody_awaits_still_runs_and_is_saved() {
    let mut r = rig(Vec::new(), &[]).await;

    drop(quick(&r.engine, INSTANT_KIND_ID).await);

    let saved = r.next_saved().await;
    assert!(
        matches!(&saved.metadata, ExecutionMetadata::QuickAction { label, .. } if label == INSTANT_KIND_ID),
        "the abandoned quick action must still record its run, got {:?}",
        saved.metadata
    );
}

#[tokio::test(start_paused = true)]
async fn a_quick_action_reports_its_outcome_without_waiting_for_its_run_to_be_saved() {
    let mut r = rig(Vec::new(), &[]).await;
    r.save_gate.send_replace(false);

    let outcome = tokio::time::timeout(
        QUIET_STEP,
        quick(&r.engine, INSTANT_KIND_ID).await.outcome(),
    )
    .await;
    assert!(
        matches!(outcome, Ok(Ok(SubActionOutcome::Success))),
        "a slow history write must not hold back the caller's outcome"
    );

    r.save_gate.send_replace(true);
    let saved = tokio::time::timeout(Duration::from_secs(5), r.saved.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        saved.metadata,
        ExecutionMetadata::QuickAction { .. }
    ));
}
