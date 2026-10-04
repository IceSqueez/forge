#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_registry::SubActionRegistry;
use forge_runtime::queue_scheduler::MAX_PENDING_PER_QUEUE;
use forge_runtime::scheduled_runs::{
    ACTION_DISABLED_REASON, ACTION_NOT_FOUND_REASON, CATCH_UP_SETTLE_LIMIT, MAX_LATE_TOLERANCE,
    MAX_PENDING_SCHEDULED_RUNS, MAX_PENDING_SCHEDULED_RUNS_PER_ACTION, MAX_SCHEDULE_DELAY,
    MAX_SCHEDULE_KEY_CHARS, MAX_SCHEDULED_ARGS_BYTES, MIN_LATE_TOLERANCE, MIN_SCHEDULE_DELAY,
    MISSED_REASON, SCHEDULED_AT_VARIABLE, SCHEDULED_DUE_AT_VARIABLE, SCHEDULED_KEY_VARIABLE,
    SCHEDULED_LATE_SECONDS_VARIABLE, SCHEDULED_RUN_DUE_KIND, UNREADABLE_REASON,
};
use forge_runtime::{
    ActionCancelRegistry, Catalog, CatchUpSettle, EventBus, HandOff, NullEventLogRepo, QueueMode,
    QueueScheduler, QueueSchedulerHandle, ScheduleDue, ScheduleError, ScheduleRequest,
    ScheduledRunsHandle, ScheduledRunsParts, SchedulerRequest, WallClock, spawn_action_engine,
    spawn_scheduled_runs,
};
use forge_storage::action::MockActionRepo;
use forge_storage::history::MockHistoryRepo;
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{
    CatalogRevision, DataProvider, MissedRunPolicy, MockScheduledRunRepo, ScheduledRun,
    ScheduledRunId, ScheduledRunRepo, ScheduledRunSpec, ScheduledRunState,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionContext, ExecutionMetadata, ExecutionMode, Queue,
    QueueId, Variant,
};
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Instant, timeout};

const DEADLINE: Duration = Duration::from_secs(5);
const BASE_UNIX_SECS: i64 = 1_791_115_200;
const FIVE_MINUTES: Duration = Duration::from_secs(5 * 60);
const ONE_SECOND: Duration = Duration::from_secs(1);

fn base() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(BASE_UNIX_SECS).unwrap()
}

struct FixedClock;

impl WallClock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        base()
    }
}

fn spec(target: ActionId, due_at: OffsetDateTime) -> ScheduledRunSpec {
    ScheduledRunSpec {
        target_action_id: target,
        due_at,
        key: None,
        missed_run_policy: MissedRunPolicy::RunLateOnce,
        args: BTreeMap::new(),
        scheduled_by_action: None,
        scheduled_by_run: None,
        trigger_event_id: None,
        scheduled_at: base() - Duration::from_secs(3_600),
        label: "follow-up".to_owned(),
    }
}

fn overdue(target: ActionId, late: Duration) -> ScheduledRunSpec {
    spec(target, base() - late)
}

fn request(target: ActionId) -> ScheduleRequest {
    ScheduleRequest {
        target_action_id: target,
        due: ScheduleDue::After(FIVE_MINUTES),
        key: None,
        missed_run_policy: MissedRunPolicy::RunLateOnce,
        args: BTreeMap::new(),
        scheduled_by_action: None,
        scheduled_by_run: None,
        trigger_event_id: None,
    }
}

fn error_kind(error: &ScheduleError) -> &'static str {
    match error {
        ScheduleError::UnknownAction => "unknown_action",
        ScheduleError::DelayTooShort => "delay_too_short",
        ScheduleError::DelayTooLong => "delay_too_long",
        ScheduleError::LateToleranceOutOfRange => "late_tolerance_out_of_range",
        ScheduleError::KeyTooLong => "key_too_long",
        ScheduleError::ArgsTooLarge { .. } => "args_too_large",
        ScheduleError::ArgsNotStorable(_) => "args_not_storable",
        ScheduleError::PendingCapReached => "pending_cap_reached",
        ScheduleError::ActionPendingCapReached => "action_pending_cap_reached",
        ScheduleError::SchedulerStopped => "scheduler_stopped",
        ScheduleError::Storage(_) => "storage",
    }
}

struct Harness {
    dp: Arc<dyn DataProvider>,
    _media: TempDir,
    bus: Arc<EventBus>,
    catalog: Arc<Catalog>,
    queues: QueueSchedulerHandle,
    queue_id: QueueId,
    runs: mpsc::UnboundedReceiver<ExecutionContext>,
}

impl Harness {
    async fn new() -> Self {
        let media = tempfile::tempdir().unwrap();
        let backend =
            SqliteBackend::open_for_test(":memory:", [0xcd; 32], media.path().join("media"), None)
                .await
                .unwrap();
        let dp: Arc<dyn DataProvider> = Arc::new(backend);
        let queue = Queue {
            id: QueueId::new(),
            name: "scheduled".to_owned(),
            description: String::new(),
            concurrency: 8,
        };
        dp.queue_repo().save(&queue).await.unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let catalog = Catalog::from_provider(dp.as_ref());
        let (runs_tx, runs) = mpsc::unbounded_channel();
        let mut history = MockHistoryRepo::new();
        history.expect_save_batch().returning(move |contexts| {
            for context in contexts {
                let _ = runs_tx.send(context.clone());
            }
            Ok(())
        });
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&catalog),
            dp.action_repo(),
            Arc::new(history),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        );
        let queues = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue.clone()]);
        Self {
            dp,
            _media: media,
            bus,
            catalog,
            queues,
            queue_id: queue.id,
            runs,
        }
    }

    fn repo(&self) -> Arc<dyn ScheduledRunRepo> {
        self.dp.scheduled_run_repo()
    }

    fn start(&self, catch_up: CatchUpSettle) -> ScheduledRunsHandle {
        spawn_scheduled_runs(ScheduledRunsParts {
            repo: self.repo(),
            revision: self.dp.scheduled_run_revision(),
            catalog: Arc::clone(&self.catalog),
            actions: self.dp.action_repo(),
            queues: self.queues.clone(),
            bus: Arc::clone(&self.bus),
            clock: Arc::new(FixedClock),
            catch_up,
        })
    }

    async fn action(&self, shape: impl FnOnce(&mut Action)) -> Action {
        let mut action = Action {
            id: ActionId::new(),
            name: "follow-up".to_owned(),
            group: None,
            queue_id: self.queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: Vec::new(),
        };
        shape(&mut action);
        self.dp.action_repo().save(&action).await.unwrap();
        action
    }

    async fn place(&self, spec: &ScheduledRunSpec) -> ScheduledRunId {
        self.repo().schedule(spec).await.unwrap().id
    }

    async fn row(&self, id: ScheduledRunId) -> ScheduledRun {
        self.repo().get(id).await.unwrap().expect("the run exists")
    }

    async fn handed_off(&self, handle: &ScheduledRunsHandle, id: ScheduledRunId) -> ScheduledRun {
        let mut changes = self.dp.scheduled_run_revision().subscribe();
        timeout(DEADLINE, async {
            while self.row(id).await.state == ScheduledRunState::Pending {
                changes.changed().await;
            }
        })
        .await
        .expect("the run never left pending");
        barrier(handle).await;
        self.row(id).await
    }

    async fn next_run(&mut self) -> ExecutionContext {
        timeout(DEADLINE, self.runs.recv())
            .await
            .expect("no run was recorded")
            .expect("the run history closed")
    }

    async fn fill_queue(&self, action: &Action, count: usize) {
        for _ in 0..count {
            self.queues
                .dispatch(SchedulerRequest {
                    queue_id: self.queue_id,
                    action_id: action.id,
                    trigger_event_id: EventId::new(),
                    trigger_kind: None,
                    initial_args: ArgStack::new(),
                    bypass_pause: false,
                })
                .await
                .unwrap();
        }
    }
}

async fn barrier(handle: &ScheduledRunsHandle) {
    let served = timeout(DEADLINE, handle.run_now(ScheduledRunId::new(i64::MAX)))
        .await
        .expect("the scheduler stopped serving commands")
        .unwrap();
    assert_eq!(served, HandOff::NotPending);
}

fn outcome(run: &ScheduledRun) -> (ScheduledRunState, Option<&str>) {
    (run.state, run.outcome_reason.as_deref())
}

#[tokio::test]
async fn an_overdue_run_meets_its_queue_mode_on_hand_off() {
    for (mode, bypass_pause, expected) in [
        (
            QueueMode::PAUSED,
            false,
            (ScheduledRunState::Failed, Some("queue_paused")),
        ),
        (
            QueueMode::DRAINING,
            false,
            (ScheduledRunState::Failed, Some("queue_draining")),
        ),
        (
            QueueMode::HOLDING,
            false,
            (ScheduledRunState::Dispatched, None),
        ),
        (
            QueueMode::PAUSED,
            true,
            (ScheduledRunState::Dispatched, None),
        ),
    ] {
        let harness = Harness::new().await;
        harness
            .queues
            .set_mode(harness.queue_id, mode)
            .await
            .unwrap();
        let action = harness
            .action(|action| action.bypass_pause = bypass_pause)
            .await;
        let id = harness.place(&overdue(action.id, FIVE_MINUTES)).await;

        let handle = harness.start(CatchUpSettle::immediately());

        let run = harness.handed_off(&handle, id).await;
        assert_eq!(
            outcome(&run),
            expected,
            "{mode:?} bypass_pause={bypass_pause}"
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum TargetShape {
    Disabled,
    Archived,
    NeverExisted,
    QueueNotRunning,
}

#[tokio::test]
async fn an_overdue_run_whose_target_cannot_run_is_settled_with_the_matching_reason() {
    for (shape, expected) in [
        (
            TargetShape::Disabled,
            (ScheduledRunState::Skipped, Some(ACTION_DISABLED_REASON)),
        ),
        (
            TargetShape::Archived,
            (ScheduledRunState::Failed, Some(ACTION_NOT_FOUND_REASON)),
        ),
        (
            TargetShape::NeverExisted,
            (ScheduledRunState::Failed, Some(ACTION_NOT_FOUND_REASON)),
        ),
        (
            TargetShape::QueueNotRunning,
            (ScheduledRunState::Failed, Some("queue_not_found")),
        ),
    ] {
        let harness = Harness::new().await;
        let target = match shape {
            TargetShape::Disabled => harness.action(|action| action.enabled = false).await.id,
            TargetShape::Archived => {
                let id = harness.action(|_| {}).await.id;
                assert!(harness.dp.action_repo().archive(id).await.unwrap());
                id
            }
            TargetShape::NeverExisted => ActionId::new(),
            TargetShape::QueueNotRunning => {
                let unscheduled = Queue {
                    id: QueueId::new(),
                    name: "not running".to_owned(),
                    description: String::new(),
                    concurrency: 1,
                };
                harness.dp.queue_repo().save(&unscheduled).await.unwrap();
                harness
                    .action(|action| action.queue_id = unscheduled.id)
                    .await
                    .id
            }
        };
        let id = harness.place(&overdue(target, FIVE_MINUTES)).await;

        let handle = harness.start(CatchUpSettle::immediately());

        let run = harness.handed_off(&handle, id).await;
        assert_eq!(outcome(&run), expected, "{shape:?}");
    }
}

#[tokio::test]
async fn an_overdue_run_is_refused_only_once_its_queue_is_full() {
    for (already_pending, expected) in [
        (
            MAX_PENDING_PER_QUEUE - 1,
            (ScheduledRunState::Dispatched, None),
        ),
        (
            MAX_PENDING_PER_QUEUE,
            (ScheduledRunState::Failed, Some("queue_pending_overflow")),
        ),
    ] {
        let harness = Harness::new().await;
        harness
            .queues
            .set_mode(harness.queue_id, QueueMode::HOLDING)
            .await
            .unwrap();
        let action = harness.action(|_| {}).await;
        harness.fill_queue(&action, already_pending).await;
        let id = harness.place(&overdue(action.id, FIVE_MINUTES)).await;

        let handle = harness.start(CatchUpSettle::immediately());

        let run = harness.handed_off(&handle, id).await;
        assert_eq!(outcome(&run), expected, "{already_pending} already pending");
    }
}

#[tokio::test]
async fn lateness_beyond_the_tolerance_skips_the_run_and_up_to_it_runs_once() {
    for (policy, late, expected) in [
        (
            MissedRunPolicy::SkipIfLateBy(MIN_LATE_TOLERANCE),
            MIN_LATE_TOLERANCE,
            (ScheduledRunState::Dispatched, None),
        ),
        (
            MissedRunPolicy::SkipIfLateBy(MIN_LATE_TOLERANCE),
            MIN_LATE_TOLERANCE + ONE_SECOND,
            (ScheduledRunState::Skipped, Some(MISSED_REASON)),
        ),
        (
            MissedRunPolicy::RunLateOnce,
            MAX_SCHEDULE_DELAY,
            (ScheduledRunState::Dispatched, None),
        ),
    ] {
        let harness = Harness::new().await;
        harness
            .queues
            .set_mode(harness.queue_id, QueueMode::HOLDING)
            .await
            .unwrap();
        let action = harness.action(|_| {}).await;
        let mut late_run = overdue(action.id, late);
        late_run.missed_run_policy = policy;
        let id = harness.place(&late_run).await;

        let handle = harness.start(CatchUpSettle::immediately());

        let run = harness.handed_off(&handle, id).await;
        assert_eq!(outcome(&run), expected, "{policy:?} late by {late:?}");
    }
}

struct ProvenanceRun {
    harness: Harness,
    context: ExecutionContext,
    run_id: ScheduledRunId,
    cause: EventId,
    scheduled_by: ActionId,
    spec: ScheduledRunSpec,
    observed: Vec<Event>,
}

const LATE_BY: Duration = Duration::from_secs(90);

async fn provenance_run() -> ProvenanceRun {
    let mut harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let cause = Event::new(
        EventSource::Twitch,
        "twitch.channel.follow",
        serde_json::Value::Null,
    );
    harness.bus.publish(cause.clone());
    let scheduled_by = ActionId::new();
    let mut late_run = overdue(action.id, LATE_BY);
    late_run.key = Some("follow:alice".to_owned());
    late_run.scheduled_by_action = Some(scheduled_by);
    late_run.scheduled_by_run = Some("run-7".to_owned());
    late_run.trigger_event_id = Some(cause.id);
    late_run.args = BTreeMap::from([
        ("greeting".to_owned(), Variant::String("hi".to_owned())),
        (
            SCHEDULED_KEY_VARIABLE.to_owned(),
            Variant::String("stale".to_owned()),
        ),
        (
            SCHEDULED_LATE_SECONDS_VARIABLE.to_owned(),
            Variant::Int(999),
        ),
    ]);
    let run_id = harness.place(&late_run).await;
    let mut subscription = harness.bus.subscribe();

    let _handle = harness.start(CatchUpSettle::immediately());

    let context = harness.next_run().await;
    let mut observed = Vec::new();
    while let Ok(Some(event)) = subscription.try_recv() {
        observed.push(event);
    }
    ProvenanceRun {
        harness,
        context,
        run_id,
        cause: cause.id,
        scheduled_by,
        spec: late_run,
        observed,
    }
}

fn scheduled_event_id(context: &ExecutionContext) -> EventId {
    match &context.metadata {
        ExecutionMetadata::Scheduled { event_id, .. } => Some(*event_id),
        _ => None,
    }
    .expect("the run carries scheduled metadata")
}

#[tokio::test]
async fn a_scheduled_run_lands_in_run_history_with_its_scheduled_origin() {
    let run = provenance_run().await;

    let origin = match run.context.metadata {
        ExecutionMetadata::Scheduled {
            scheduled_run_id,
            scheduled_by_action,
            scheduled_by_run,
            ..
        } => Some((scheduled_run_id, scheduled_by_action, scheduled_by_run)),
        _ => None,
    };
    assert_eq!(
        origin,
        Some((
            run.run_id.get(),
            Some(run.scheduled_by),
            Some("run-7".to_owned())
        ))
    );
}

#[tokio::test]
async fn the_run_trigger_is_a_recorded_due_event_caused_by_the_original_trigger() {
    let run = provenance_run().await;

    let recorded = run
        .harness
        .bus
        .lookup(scheduled_event_id(&run.context))
        .expect("the due event is in the lineage ring");

    assert_eq!(
        (recorded.kind.as_str(), recorded.caused_by),
        (SCHEDULED_RUN_DUE_KIND, Some(run.cause))
    );
}

#[tokio::test]
async fn the_recorded_due_event_never_reaches_bus_subscribers() {
    let run = provenance_run().await;

    let delivered: Vec<&str> = run
        .observed
        .iter()
        .map(|event| event.kind.as_str())
        .filter(|kind| *kind == SCHEDULED_RUN_DUE_KIND)
        .collect();

    assert!(delivered.is_empty(), "{delivered:?}");
}

#[tokio::test]
async fn provenance_variables_override_same_named_snapshot_entries() {
    let run = provenance_run().await;

    let seen: BTreeMap<&str, Option<Variant>> = [
        "greeting",
        SCHEDULED_AT_VARIABLE,
        SCHEDULED_DUE_AT_VARIABLE,
        SCHEDULED_LATE_SECONDS_VARIABLE,
        SCHEDULED_KEY_VARIABLE,
    ]
    .into_iter()
    .map(|name| (name, run.context.arg_stack_snapshot.get(name).cloned()))
    .collect();

    let expected = BTreeMap::from([
        ("greeting", Some(Variant::String("hi".to_owned()))),
        (
            SCHEDULED_AT_VARIABLE,
            Some(Variant::Datetime(run.spec.scheduled_at)),
        ),
        (
            SCHEDULED_DUE_AT_VARIABLE,
            Some(Variant::Datetime(run.spec.due_at)),
        ),
        (
            SCHEDULED_LATE_SECONDS_VARIABLE,
            Some(Variant::Int(i64::try_from(LATE_BY.as_secs()).unwrap())),
        ),
        (
            SCHEDULED_KEY_VARIABLE,
            Some(Variant::String("follow:alice".to_owned())),
        ),
    ]);
    assert_eq!(seen, expected);
}

#[tokio::test]
async fn a_triggered_run_still_records_trigger_metadata() {
    let mut harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let trigger = EventId::new();

    harness
        .queues
        .dispatch(SchedulerRequest {
            queue_id: harness.queue_id,
            action_id: action.id,
            trigger_event_id: trigger,
            trigger_kind: Some("twitch.channel.follow".to_owned()),
            initial_args: ArgStack::new(),
            bypass_pause: false,
        })
        .await
        .unwrap();

    let context = harness.next_run().await;
    assert_eq!(
        context.metadata,
        ExecutionMetadata::Trigger {
            event_id: trigger,
            trigger_kind: Some("twitch.channel.follow".to_owned()),
        }
    );
}

#[tokio::test]
async fn run_now_dispatches_a_pending_run_before_it_is_due() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let mut future = spec(action.id, base() + MAX_SCHEDULE_DELAY);
    future.missed_run_policy = MissedRunPolicy::SkipIfLateBy(MIN_LATE_TOLERANCE);
    let id = harness.place(&future).await;
    let handle = harness.start(CatchUpSettle::immediately());

    let handed = timeout(DEADLINE, handle.run_now(id))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        (handed, harness.row(id).await.state),
        (HandOff::Dispatched, ScheduledRunState::Dispatched)
    );
}

#[tokio::test]
async fn run_now_hands_off_with_zero_lateness_and_an_empty_key() {
    let mut harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let id = harness
        .place(&spec(action.id, base() + MIN_SCHEDULE_DELAY))
        .await;
    let handle = harness.start(CatchUpSettle::immediately());

    timeout(DEADLINE, handle.run_now(id))
        .await
        .unwrap()
        .unwrap();

    let context = harness.next_run().await;
    let provenance = (
        context
            .arg_stack_snapshot
            .get(SCHEDULED_LATE_SECONDS_VARIABLE),
        context.arg_stack_snapshot.get(SCHEDULED_KEY_VARIABLE),
    );
    assert_eq!(
        provenance,
        (
            Some(&Variant::Int(0)),
            Some(&Variant::String(String::new()))
        )
    );
}

#[tokio::test]
async fn run_now_reports_not_pending_for_a_run_that_was_cancelled_or_already_handed_off() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let handle = harness.start(CatchUpSettle::immediately());
    let cancelled = harness.place(&spec(action.id, base() + FIVE_MINUTES)).await;
    assert!(handle.cancel(cancelled).await.unwrap());
    let handed = harness.place(&spec(action.id, base() + FIVE_MINUTES)).await;
    assert_eq!(handle.run_now(handed).await.unwrap(), HandOff::Dispatched);

    for (case, id) in [("cancelled", cancelled), ("handed off", handed)] {
        let again = timeout(DEADLINE, handle.run_now(id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(again, HandOff::NotPending, "{case}");
    }
}

#[tokio::test]
async fn schedule_places_the_run_only_for_delays_within_the_bounds() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let handle = harness.start(CatchUpSettle::immediately());

    for (due, expected) in [
        (
            ScheduleDue::After(MIN_SCHEDULE_DELAY - ONE_SECOND),
            Err("delay_too_short"),
        ),
        (
            ScheduleDue::After(MIN_SCHEDULE_DELAY),
            Ok(base() + MIN_SCHEDULE_DELAY),
        ),
        (
            ScheduleDue::After(MAX_SCHEDULE_DELAY),
            Ok(base() + MAX_SCHEDULE_DELAY),
        ),
        (
            ScheduleDue::After(MAX_SCHEDULE_DELAY + ONE_SECOND),
            Err("delay_too_long"),
        ),
        (ScheduleDue::At(base() - ONE_SECOND), Err("delay_too_short")),
        (
            ScheduleDue::At(base() + MIN_SCHEDULE_DELAY - ONE_SECOND),
            Err("delay_too_short"),
        ),
        (
            ScheduleDue::At(base() + MIN_SCHEDULE_DELAY),
            Ok(base() + MIN_SCHEDULE_DELAY),
        ),
        (
            ScheduleDue::At(base() + MAX_SCHEDULE_DELAY + ONE_SECOND),
            Err("delay_too_long"),
        ),
    ] {
        let placed = handle
            .schedule(ScheduleRequest {
                due,
                ..request(action.id)
            })
            .await
            .map(|placement| placement.due_at)
            .map_err(|error| error_kind(&error));
        assert_eq!(placed, expected, "{due:?}");
    }
}

#[tokio::test]
async fn schedule_accepts_a_late_tolerance_only_within_the_bounds() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let handle = harness.start(CatchUpSettle::immediately());

    for (tolerance, accepted) in [
        (MIN_LATE_TOLERANCE - ONE_SECOND, false),
        (MIN_LATE_TOLERANCE, true),
        (MAX_LATE_TOLERANCE, true),
        (MAX_LATE_TOLERANCE + ONE_SECOND, false),
    ] {
        let placed = handle
            .schedule(ScheduleRequest {
                missed_run_policy: MissedRunPolicy::SkipIfLateBy(tolerance),
                ..request(action.id)
            })
            .await
            .map(|_| ())
            .map_err(|error| error_kind(&error));
        let expected = if accepted {
            Ok(())
        } else {
            Err("late_tolerance_out_of_range")
        };
        assert_eq!(placed, expected, "{tolerance:?}");
    }
}

#[tokio::test]
async fn schedule_stores_a_trimmed_key_counted_in_characters() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let handle = harness.start(CatchUpSettle::immediately());
    let longest = "k".repeat(MAX_SCHEDULE_KEY_CHARS);
    let longest_cyrillic = "ї".repeat(MAX_SCHEDULE_KEY_CHARS);

    for (key, expected) in [
        (longest.clone(), Ok(Some(longest.clone()))),
        (longest_cyrillic.clone(), Ok(Some(longest_cyrillic.clone()))),
        (format!("{longest}k"), Err("key_too_long")),
        (format!("  {longest}  "), Ok(Some(longest.clone()))),
        ("   ".to_owned(), Ok(None)),
    ] {
        let placed = handle
            .schedule(ScheduleRequest {
                key: Some(key.clone()),
                ..request(action.id)
            })
            .await
            .map_err(|error| error_kind(&error));
        let stored = match placed {
            Ok(placement) => Ok(harness.row(placement.id).await.spec.key),
            Err(kind) => Err(kind),
        };
        assert_eq!(stored, expected, "{key:?}");
    }
}

#[tokio::test]
async fn schedule_refuses_captured_variables_one_byte_over_the_limit() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let handle = harness.start(CatchUpSettle::immediately());
    let filler =
        |len: usize| BTreeMap::from([("blob".to_owned(), Variant::String("x".repeat(len)))]);
    let overhead = serde_json::to_vec(&filler(0)).unwrap().len();
    let at_limit = MAX_SCHEDULED_ARGS_BYTES - overhead;

    for (len, expected) in [
        (at_limit, Ok(())),
        (at_limit + 1, Err(Some(MAX_SCHEDULED_ARGS_BYTES + 1))),
    ] {
        let placed = handle
            .schedule(ScheduleRequest {
                args: filler(len),
                ..request(action.id)
            })
            .await
            .map(|_| ())
            .map_err(|error| match error {
                ScheduleError::ArgsTooLarge { bytes } => Some(bytes),
                _ => None,
            });
        assert_eq!(placed, expected, "{len} filler bytes");
    }
}

#[tokio::test]
async fn schedule_refuses_a_target_that_is_missing_or_archived() {
    let harness = Harness::new().await;
    let archived = harness.action(|_| {}).await.id;
    assert!(harness.dp.action_repo().archive(archived).await.unwrap());
    let disabled = harness.action(|action| action.enabled = false).await.id;
    let handle = harness.start(CatchUpSettle::immediately());

    for (case, target, expected) in [
        ("missing", ActionId::new(), Err("unknown_action")),
        ("archived", archived, Err("unknown_action")),
        ("disabled", disabled, Ok(())),
    ] {
        let placed = handle
            .schedule(request(target))
            .await
            .map(|_| ())
            .map_err(|error| error_kind(&error));
        assert_eq!(placed, expected, "{case}");
    }
}

async fn place_pending(harness: &Harness, target: ActionId, count: u64, key: Option<&str>) {
    for _ in 0..count {
        harness
            .place(&spec(target, base() + MAX_SCHEDULE_DELAY))
            .await;
    }
    if let Some(key) = key {
        let mut keyed = spec(target, base() + MAX_SCHEDULE_DELAY);
        keyed.key = Some(key.to_owned());
        harness.place(&keyed).await;
    }
}

#[tokio::test]
async fn schedule_enforces_the_per_action_cap_except_for_a_same_action_keyed_replace() {
    let cap = MAX_PENDING_SCHEDULED_RUNS_PER_ACTION;
    for (case, unkeyed, key_held_by_target, request_key, expected) in [
        ("one under the cap", cap - 1, None, None, Ok(())),
        (
            "at the cap",
            cap,
            None,
            None,
            Err("action_pending_cap_reached"),
        ),
        (
            "at the cap replacing its own keyed run",
            cap - 1,
            Some(true),
            Some("k"),
            Ok(()),
        ),
        (
            "at the cap replacing another action's keyed run",
            cap,
            Some(false),
            Some("k"),
            Err("action_pending_cap_reached"),
        ),
    ] {
        let harness = Harness::new().await;
        let action = harness.action(|_| {}).await;
        place_pending(
            &harness,
            action.id,
            unkeyed,
            key_held_by_target.filter(|own| *own).map(|_| "k"),
        )
        .await;
        if key_held_by_target == Some(false) {
            place_pending(&harness, ActionId::new(), 0, Some("k")).await;
        }
        let handle = harness.start(CatchUpSettle::immediately());

        let placed = handle
            .schedule(ScheduleRequest {
                key: request_key.map(str::to_owned),
                ..request(action.id)
            })
            .await
            .map(|_| ())
            .map_err(|error| error_kind(&error));
        assert_eq!(placed, expected, "{case}");
    }
}

#[tokio::test]
async fn schedule_enforces_the_global_cap_except_for_a_keyed_replace() {
    let cap = MAX_PENDING_SCHEDULED_RUNS;
    for (case, others, keyed_other, request_key, expected) in [
        ("one under the cap", cap - 1, false, None, Ok(())),
        ("at the cap", cap, false, None, Err("pending_cap_reached")),
        (
            "at the cap replacing a keyed run",
            cap - 1,
            true,
            Some("k"),
            Ok(()),
        ),
    ] {
        let harness = Harness::new().await;
        let action = harness.action(|_| {}).await;
        let others_per_target = cap / 10;
        for _ in 0..others / others_per_target {
            place_pending(&harness, ActionId::new(), others_per_target, None).await;
        }
        place_pending(
            &harness,
            ActionId::new(),
            others % others_per_target,
            keyed_other.then_some("k"),
        )
        .await;
        let handle = harness.start(CatchUpSettle::immediately());

        let placed = handle
            .schedule(ScheduleRequest {
                key: request_key.map(str::to_owned),
                ..request(action.id)
            })
            .await
            .map(|_| ())
            .map_err(|error| error_kind(&error));
        assert_eq!(placed, expected, "{case}");
    }
}

#[tokio::test]
async fn overdue_runs_wait_for_integrations_to_settle_before_dispatch() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let id = harness.place(&overdue(action.id, FIVE_MINUTES)).await;
    let (release, settled) = oneshot::channel::<()>();
    let handle = harness.start(CatchUpSettle::when(async move {
        let _ = settled.await;
    }));
    for _ in 0..8 {
        barrier(&handle).await;
    }
    let before_settle = harness.row(id).await.state;

    release.send(()).unwrap();

    let after_settle = harness.handed_off(&handle, id).await.state;
    assert_eq!(
        (before_settle, after_settle),
        (ScheduledRunState::Pending, ScheduledRunState::Dispatched)
    );
}

#[tokio::test]
async fn schedule_and_run_now_are_served_while_catch_up_waits() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let (_never_settles, settled) = oneshot::channel::<()>();
    let handle = harness.start(CatchUpSettle::when(async move {
        let _ = settled.await;
    }));

    let placed = timeout(DEADLINE, handle.schedule(request(action.id)))
        .await
        .unwrap()
        .unwrap();
    let handed = timeout(DEADLINE, handle.run_now(placed.id))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(handed, HandOff::Dispatched);
}

#[tokio::test]
async fn catch_up_hands_off_overdue_runs_by_due_time_then_insertion_order() {
    let harness = Harness::new().await;
    harness
        .queues
        .set_mode(harness.queue_id, QueueMode::HOLDING)
        .await
        .unwrap();
    let action = harness.action(|_| {}).await;
    let one_minute = Duration::from_secs(60);
    let newest = harness.place(&overdue(action.id, one_minute)).await;
    let oldest = harness.place(&overdue(action.id, one_minute * 3)).await;
    let middle = harness.place(&overdue(action.id, one_minute * 2)).await;
    let oldest_tie = harness.place(&overdue(action.id, one_minute * 3)).await;

    let handle = harness.start(CatchUpSettle::immediately());
    for id in [newest, oldest, middle, oldest_tie] {
        harness.handed_off(&handle, id).await;
    }

    let mut order: Vec<i64> = harness
        .bus
        .recent(16)
        .into_iter()
        .filter(|event| event.kind == SCHEDULED_RUN_DUE_KIND)
        .filter_map(|event| event.payload["scheduled_run_id"].as_i64())
        .collect();
    order.reverse();
    assert_eq!(
        order,
        [oldest, oldest_tie, middle, newest].map(ScheduledRunId::get)
    );
}

#[tokio::test]
async fn a_run_placed_behind_the_scheduler_is_picked_up_on_the_revision_alone() {
    let harness = Harness::new().await;
    let action = harness.action(|_| {}).await;
    let handle = harness.start(CatchUpSettle::immediately());
    barrier(&handle).await;

    let id = harness.place(&overdue(action.id, FIVE_MINUTES)).await;

    assert_eq!(
        harness.handed_off(&handle, id).await.state,
        ScheduledRunState::Dispatched
    );
}

fn mock_parts(
    repo: MockScheduledRunRepo,
    revision: CatalogRevision,
    catch_up: CatchUpSettle,
) -> ScheduledRunsParts {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let catalog = Catalog::new(
        Arc::new(MockActionRepo::new()),
        Arc::new(MockTriggerInstanceRepo::new()),
        CatalogRevision::new(),
    );
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&catalog),
        Arc::new(MockActionRepo::new()),
        Arc::new(MockHistoryRepo::new()),
        Arc::new(SubActionRegistry::new()),
        Arc::new(ActionCancelRegistry::new()),
    );
    ScheduledRunsParts {
        repo: Arc::new(repo),
        revision,
        catalog,
        actions: Arc::new(MockActionRepo::new()),
        queues: QueueScheduler::spawn(engine, Arc::clone(&bus), Vec::new()),
        bus,
        clock: Arc::new(FixedClock),
        catch_up,
    }
}

#[tokio::test(start_paused = true)]
async fn catch_up_starts_at_the_settle_limit_when_integrations_never_settle() {
    let (passes_tx, mut passes) = mpsc::unbounded_channel();
    let mut repo = MockScheduledRunRepo::new();
    repo.expect_next_due()
        .returning(|| Ok(Some(base() - FIVE_MINUTES)));
    repo.expect_fail_unreadable_due().returning(move |_, _| {
        let _ = passes_tx.send(Instant::now());
        Ok(Vec::new())
    });
    repo.expect_list_due().returning(|_| Ok(Vec::new()));
    let began = Instant::now();

    let _handle = spawn_scheduled_runs(mock_parts(
        repo,
        CatalogRevision::new(),
        CatchUpSettle::when(std::future::pending()),
    ));

    let first_pass = timeout(CATCH_UP_SETTLE_LIMIT * 2, passes.recv())
        .await
        .expect("catch-up never started")
        .unwrap();
    let waited = first_pass - began;
    assert!(
        (CATCH_UP_SETTLE_LIMIT..CATCH_UP_SETTLE_LIMIT + ONE_SECOND).contains(&waited),
        "{waited:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn an_idle_scheduler_reads_the_next_due_time_only_at_boot_and_on_a_revision() {
    let reads = Arc::new(AtomicUsize::new(0));
    let mut repo = MockScheduledRunRepo::new();
    let counted = Arc::clone(&reads);
    repo.expect_next_due().returning(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    });
    let revision = CatalogRevision::new();
    let _handle = spawn_scheduled_runs(mock_parts(
        repo,
        revision.clone(),
        CatchUpSettle::immediately(),
    ));

    tokio::time::advance(Duration::from_secs(24 * 60 * 60)).await;
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
    let idle = reads.load(Ordering::SeqCst);
    revision.advance();
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }

    assert_eq!((idle, reads.load(Ordering::SeqCst)), (1, 2));
}

#[tokio::test(start_paused = true)]
async fn a_dispatch_pass_fails_unreadable_due_runs_at_the_wall_clock_time() {
    let (calls_tx, mut calls) = mpsc::unbounded_channel();
    let mut repo = MockScheduledRunRepo::new();
    repo.expect_next_due()
        .returning(|| Ok(Some(base() - FIVE_MINUTES)));
    repo.expect_fail_unreadable_due()
        .returning(move |now, reason| {
            let _ = calls_tx.send((now, reason.to_owned()));
            Ok(Vec::new())
        });
    repo.expect_list_due().returning(|_| Ok(Vec::new()));

    let _handle = spawn_scheduled_runs(mock_parts(
        repo,
        CatalogRevision::new(),
        CatchUpSettle::immediately(),
    ));

    let first = timeout(DEADLINE, calls.recv()).await.unwrap().unwrap();
    assert_eq!(first, (base(), UNREADABLE_REASON.to_owned()));
}
