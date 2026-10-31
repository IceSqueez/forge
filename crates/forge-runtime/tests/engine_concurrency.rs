#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::repos::InMemoryActionRepo;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_events::Event;
use forge_events::EventSubscription;
use forge_registry::{
    FormField, RegistryError, RunContext, StepTimer, SubActionCategory, SubActionRegistry,
    SubActionRunner,
};
use forge_runtime::{ACTION_DISABLED_REASON, ACTION_NOT_FOUND_REASON, ACTIONS_UNAVAILABLE_REASON};
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, Catalog, DispatchError, ENGINE_STOPPED_REASON,
    EventBus, ExecutionRequest, MAX_PENDING_PER_QUEUE, NullEventLogRepo, PendingQuickAction,
    QUEUE_DRAINING_REASON, QUEUE_PAUSED_REASON, QueueAdmission, QueueMode, QueueRuntimeState,
    QueueScheduler, QueueSchedulerHandle, SchedulerRequest, spawn_action_engine,
};
use forge_runtime::{CoreLogicWaitRunner, WAIT_KIND_ID, WAIT_MS_KEY};
use forge_storage::MockTriggerInstanceRepo;
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
    for a in actions {
        repo.save(a).await.unwrap();
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
        repo: repo_handle,
        until_cancelled_running,
    }
}

fn req(a: &Action) -> SchedulerRequest {
    SchedulerRequest {
        queue_id: a.queue_id,
        action_id: a.id,
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
            |o| matches!(o, SubActionOutcome::Failed(reason) if reason.code_or_text() == FAILURE_REASON),
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
async fn shutdown_cancels_a_running_quick_action() {
    let r = rig(Vec::new(), &[]).await;
    let pending = quick(&r.engine, UNTIL_CANCELLED_KIND_ID).await;
    r.until_cancelled_running.notified().await;

    r.engine.clone().shutdown();

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

fn reason_of(ev: &Event) -> Option<&str> {
    ev.payload.get("reason").and_then(|v| v.as_str())
}

fn queue_of(ev: &Event) -> Option<&str> {
    ev.payload.get("queue_id").and_then(|v| v.as_str())
}

async fn refused_run(
    r: &mut Rig,
    a: &Action,
    sub: &mut EventSubscription,
) -> (Event, ExecutionContext) {
    r.sched.dispatch(req(a)).await.unwrap();
    let skipped = events_until(sub, |ev| ev.kind == "action.skipped")
        .await
        .pop()
        .unwrap();
    (skipped, r.next_saved().await)
}

fn failure_code(saved: &ExecutionContext) -> String {
    match &saved.outcome {
        ExecutionOutcome::Failed(reason) => reason.code_or_text(),
        other => panic!("a refused run is saved as failed, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_run_of_a_disabled_action_is_skipped_and_saved_as_failed_on_its_own_queue() {
    let q = QueueId::new();
    let disabled = Action {
        enabled: false,
        ..single(q, INSTANT_KIND_ID)
    };
    let mut r = rig(vec![queue(q, 1)], &[&disabled]).await;
    let mut sub = r.bus.subscribe();

    let (skipped, saved) = refused_run(&mut r, &disabled, &mut sub).await;

    assert_eq!(reason_of(&skipped), Some(ACTION_DISABLED_REASON));
    assert_eq!(queue_of(&skipped), Some(q.to_string().as_str()));
    assert_eq!(failure_code(&saved), "core.action_disabled");
    assert!(saved.telemetry.is_empty(), "no step of a refused run runs");
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_run_of_a_deleted_action_is_skipped_and_saved_as_failed() {
    let q = QueueId::new();
    let deleted = single(q, INSTANT_KIND_ID);
    let mut r = rig(vec![queue(q, 1)], &[]).await;
    let mut sub = r.bus.subscribe();

    let (skipped, saved) = refused_run(&mut r, &deleted, &mut sub).await;

    assert_eq!(reason_of(&skipped), Some(ACTION_NOT_FOUND_REASON));
    assert_eq!(
        queue_of(&skipped),
        Some(DEFAULT_QUEUE_ID.to_string().as_str())
    );
    assert_eq!(failure_code(&saved), "core.unknown_action");
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_run_is_skipped_and_saved_as_failed_when_actions_cannot_be_read() {
    let q = QueueId::new();
    let wanted = single(q, INSTANT_KIND_ID);
    let mut r = rig(vec![queue(q, 1)], &[&wanted]).await;
    r.repo.set_list_unreadable(true);
    let mut sub = r.bus.subscribe();

    let (skipped, saved) = refused_run(&mut r, &wanted, &mut sub).await;

    assert_eq!(reason_of(&skipped), Some(ACTIONS_UNAVAILABLE_REASON));
    assert_eq!(failure_code(&saved), "core.storage_failed");
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_lookup_failure_for_an_action_outside_the_catalog_is_saved_as_a_storage_failure() {
    let q = QueueId::new();
    let disabled = Action {
        enabled: false,
        ..single(q, INSTANT_KIND_ID)
    };
    let mut r = rig(vec![queue(q, 1)], &[&disabled]).await;
    r.repo.set_get_unreadable(true);
    let mut sub = r.bus.subscribe();

    let (skipped, saved) = refused_run(&mut r, &disabled, &mut sub).await;

    assert_eq!(reason_of(&skipped), Some(ACTIONS_UNAVAILABLE_REASON));
    assert_eq!(failure_code(&saved), "core.storage_failed");
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_queued_run_that_cannot_reach_a_stopped_engine_is_skipped_and_frees_its_slot() {
    let q = QueueId::new();
    let a = single(q, INSTANT_KIND_ID);
    let r = rig(vec![queue(q, 1)], &[&a]).await;
    let mut sub = r.bus.subscribe();
    r.engine.clone().shutdown();
    while r
        .engine
        .dispatch(ExecutionRequest {
            action_id: a.id,
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

    r.sched.dispatch(req(&a)).await.unwrap();

    let skipped = events_until(&mut sub, |ev| ev.kind == "action.skipped")
        .await
        .pop()
        .unwrap();
    assert_eq!(reason_of(&skipped), Some(ENGINE_STOPPED_REASON));
    settle(&r.sched, q, "the undelivered run's slot released", |s| {
        s.in_flight == 0
    })
    .await;
    r.sched.shutdown();
}

fn concurrent(action: Action) -> Action {
    Action {
        concurrent: true,
        ..action
    }
}

fn admitted(a: &Action) -> SchedulerRequest {
    SchedulerRequest {
        admission: QueueAdmission::from(a),
        ..req(a)
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
    let q = QueueId::new();
    let slow = single(q, SLOW_KIND_ID);
    let beside = concurrent(single(q, INSTANT_KIND_ID));
    let r = rig(vec![queue(q, 1)], &[&slow, &beside]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(req(&slow)).await.unwrap();
    events_until(&mut sub, start_of(&slow)).await;
    r.sched.dispatch(admitted(&beside)).await.unwrap();

    let seen = events_until(&mut sub, done_of(&beside)).await;
    assert!(
        !ids_of(&seen, "action.done").contains(&slow.id.to_string()),
        "the concurrent action must finish while the slow one still holds the only slot"
    );
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn alongside_runs_count_as_in_flight_beyond_the_queue_limit() {
    let q = QueueId::new();
    let slow = single(q, SLOW_KIND_ID);
    let beside = concurrent(single(q, SLOW_KIND_ID));
    let r = rig(vec![queue(q, 1)], &[&slow, &beside]).await;

    r.sched.dispatch(req(&slow)).await.unwrap();
    r.sched.dispatch(admitted(&beside)).await.unwrap();

    settle(&r.sched, q, "both runs in flight on a queue of one", |s| {
        s.in_flight == 2 && s.pending == 0
    })
    .await;
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_running_alongside_run_leaves_the_slot_to_a_gated_run() {
    let q = QueueId::new();
    let beside = concurrent(single(q, SLOW_KIND_ID));
    let gated = single(q, INSTANT_KIND_ID);
    let r = rig(vec![queue(q, 1)], &[&beside, &gated]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(admitted(&beside)).await.unwrap();
    events_until(&mut sub, start_of(&beside)).await;
    r.sched.dispatch(req(&gated)).await.unwrap();

    let seen = events_until(&mut sub, done_of(&gated)).await;
    assert!(
        !ids_of(&seen, "action.done").contains(&beside.id.to_string()),
        "the gated action must take the slot while the concurrent one still runs"
    );
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_concurrent_action_runs_its_steps_one_after_another() {
    let q = QueueId::new();
    let both = concurrent(action(
        q,
        vec![
            step(SLOW_KIND_ID, SubActionConfig::new()),
            step(INSTANT_KIND_ID, SubActionConfig::new()),
        ],
    ));
    let r = rig(vec![queue(q, 1)], &[&both]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(admitted(&both)).await.unwrap();
    let mut seen = events_until(&mut sub, start_of(&both)).await;
    quiesce().await;
    seen.extend(drain(&mut sub));
    r.open_slow_gate();
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
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn holding_withholds_an_alongside_run_until_the_queue_resumes() {
    let q = QueueId::new();
    let beside = concurrent(single(q, INSTANT_KIND_ID));
    let r = rig(vec![queue(q, 1)], &[&beside]).await;
    let mut sub = r.bus.subscribe();

    r.sched.set_mode(q, QueueMode::HOLDING).await.unwrap();
    r.sched.dispatch(admitted(&beside)).await.unwrap();
    settle(&r.sched, q, "the run held", |s| s.pending == 1).await;
    quiesce().await;
    let while_held = drain(&mut sub);
    r.sched.set_mode(q, QueueMode::RUNNING).await.unwrap();
    let after_resume = events_until(&mut sub, done_of(&beside)).await;

    assert_eq!(
        (
            ids_of(&while_held, "action.start"),
            ids_of(&after_resume, "action.start")
        ),
        (Vec::new(), vec![beside.id.to_string()])
    );
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_bypass_pause_alongside_run_passes_a_held_queue_while_a_plain_one_waits() {
    let q = QueueId::new();
    let plain = concurrent(single(q, INSTANT_KIND_ID));
    let bypass = Action {
        bypass_pause: true,
        ..concurrent(single(q, INSTANT_KIND_ID))
    };
    let r = rig(vec![queue(q, 1)], &[&plain, &bypass]).await;
    let mut sub = r.bus.subscribe();

    r.sched.set_mode(q, QueueMode::HOLDING).await.unwrap();
    r.sched.dispatch(admitted(&plain)).await.unwrap();
    r.sched.dispatch(admitted(&bypass)).await.unwrap();
    let seen = events_until(&mut sub, done_of(&bypass)).await;

    assert_eq!(
        (
            ids_of(&seen, "action.start"),
            state_of(&r.sched, q).await.pending
        ),
        (vec![bypass.id.to_string()], 1)
    );
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn paused_and_draining_queues_refuse_an_alongside_run() {
    let q = QueueId::new();
    let beside = concurrent(single(q, INSTANT_KIND_ID));
    let r = rig(vec![queue(q, 1)], &[&beside]).await;
    let mut sub = r.bus.subscribe();

    for (mode, reason) in [
        (QueueMode::PAUSED, QUEUE_PAUSED_REASON),
        (QueueMode::DRAINING, QUEUE_DRAINING_REASON),
    ] {
        r.sched.set_mode(q, mode).await.unwrap();
        r.sched.dispatch(admitted(&beside)).await.unwrap();
        let seen = events_until(&mut sub, |ev| ev.kind == "action.skipped").await;
        assert_eq!(
            seen.last()
                .and_then(|ev| ev.payload.get("reason"))
                .and_then(|v| v.as_str()),
            Some(reason),
            "{mode:?}"
        );
    }
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn an_alongside_run_past_the_pending_cap_is_refused_as_overflow() {
    let q = QueueId::new();
    let plain = single(q, INSTANT_KIND_ID);
    let beside = concurrent(single(q, INSTANT_KIND_ID));
    let r = rig(vec![queue(q, 1)], &[&plain, &beside]).await;

    r.sched.set_mode(q, QueueMode::HOLDING).await.unwrap();
    for _ in 0..MAX_PENDING_PER_QUEUE {
        r.sched.dispatch(req(&plain)).await.unwrap();
    }
    r.sched.dispatch(admitted(&beside)).await.unwrap();

    settle(&r.sched, q, "the alongside run counted as overflow", |s| {
        s.pending == MAX_PENDING_PER_QUEUE && s.overflowed == 1
    })
    .await;
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn clearing_discards_withheld_alongside_runs() {
    let q = QueueId::new();
    let beside = concurrent(single(q, INSTANT_KIND_ID));
    let r = rig(vec![queue(q, 1)], &[&beside]).await;
    let mut sub = r.bus.subscribe();

    r.sched.set_mode(q, QueueMode::HOLDING).await.unwrap();
    for _ in 0..2 {
        r.sched.dispatch(admitted(&beside)).await.unwrap();
    }
    settle(&r.sched, q, "two runs held", |s| s.pending == 2).await;
    r.sched.clear(q, true).await.unwrap();
    r.sched.set_mode(q, QueueMode::RUNNING).await.unwrap();
    quiesce().await;

    assert!(
        ids_of(&drain(&mut sub), "action.start").is_empty(),
        "cleared alongside runs must never start"
    );
    r.sched.shutdown();
}

#[tokio::test(start_paused = true)]
async fn clearing_without_keeping_current_cancels_a_running_alongside_run() {
    let q = QueueId::new();
    let waiting = concurrent(long_wait_then_instant(q));
    let mut r = rig(vec![queue(q, 1)], &[&waiting]).await;
    let mut sub = r.bus.subscribe();

    r.sched.dispatch(admitted(&waiting)).await.unwrap();
    events_until(&mut sub, start_of(&waiting)).await;
    r.sched.clear(q, false).await.unwrap();

    assert_eq!(r.next_saved().await.outcome, ExecutionOutcome::Cancelled);
    r.sched.shutdown();
}
