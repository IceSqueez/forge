#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRegistry, SubActionRunner, effective_config};
use forge_runtime::scheduled_runs::{MAX_SCHEDULED_ARGS_BYTES, MIN_LATE_TOLERANCE};
use forge_runtime::sub_action_runners::{
    CANCEL_SCHEDULED_KIND_ID, CoreActionCancelScheduledRunner, CoreActionRunRunner,
    CoreActionScheduleRunner, SCHEDULE_ACTION_KIND_ID,
};
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, Catalog, CatchUpSettle, EventBus, ExecutionRequest,
    NullEventLogRepo, QueueScheduler, ScheduleError, ScheduledRunsCell, ScheduledRunsParts,
    WallClock, spawn_action_engine, spawn_scheduled_runs,
};
use forge_storage::history::MockHistoryRepo;
use forge_storage::{DataProvider, MissedRunPolicy, ScheduledRun};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionContext, ExecutionMode, Queue, QueueId,
    SubActionConfig, SubActionOutcome, SubActionStep, Variant,
};
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(5);
const BASE_UNIX_SECS: i64 = 1_791_115_200;
const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(60 * 60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn base() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(BASE_UNIX_SECS).unwrap()
}

struct FixedClock;

impl WallClock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        base()
    }
}

struct NullPublisher;

impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

fn text(value: &str) -> Variant {
    Variant::String(value.to_owned())
}

fn config(runner: &dyn SubActionRunner, pairs: &[(&str, Variant)]) -> SubActionConfig {
    let overrides: SubActionConfig = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect();
    effective_config(&runner.default_config(), &overrides)
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

fn failure(outcome: &SubActionOutcome) -> Option<&str> {
    match outcome {
        SubActionOutcome::Failed(reason) => Some(reason.as_str()),
        _ => None,
    }
}

struct Harness {
    dp: Arc<dyn DataProvider>,
    _media: TempDir,
    bus: Arc<EventBus>,
    engine: ActionEngineHandle,
    queue_id: QueueId,
    cell: ScheduledRunsCell,
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
        let cell = ScheduledRunsCell::new();
        let mut registry = SubActionRegistry::new();
        registry
            .register(Box::new(CoreActionScheduleRunner::new(cell.clone())))
            .unwrap();
        registry
            .register(Box::new(CoreActionRunRunner::new(dp.action_repo())))
            .unwrap();
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
            Arc::new(registry),
            Arc::new(ActionCancelRegistry::new()),
        );
        let queues = QueueScheduler::spawn(engine.clone(), Arc::clone(&bus), vec![queue.clone()]);
        cell.set(spawn_scheduled_runs(ScheduledRunsParts {
            repo: dp.scheduled_run_repo(),
            revision: dp.scheduled_run_revision(),
            catalog,
            actions: dp.action_repo(),
            queues,
            bus: Arc::clone(&bus),
            clock: Arc::new(FixedClock),
            catch_up: CatchUpSettle::immediately(),
        }));
        Self {
            dp,
            _media: media,
            bus,
            engine,
            queue_id: queue.id,
            cell,
            runs,
        }
    }

    async fn action(&self, concurrent: bool, sub_actions: Vec<SubActionStep>) -> Action {
        let action = Action {
            id: ActionId::new(),
            name: "scheduling".to_owned(),
            group: None,
            queue_id: self.queue_id,
            enabled: true,
            concurrent,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions,
        };
        self.dp.action_repo().save(&action).await.unwrap();
        action
    }

    async fn target(&self) -> ActionId {
        self.action(false, Vec::new()).await.id
    }

    fn scheduler(&self) -> CoreActionScheduleRunner {
        CoreActionScheduleRunner::new(self.cell.clone())
    }

    fn canceller(&self) -> CoreActionCancelScheduledRunner {
        CoreActionCancelScheduledRunner::new(self.cell.clone())
    }

    async fn schedule(
        &self,
        stack: &ArgStack,
        pairs: &[(&str, Variant)],
    ) -> (SubActionOutcome, Option<ArgStack>) {
        let runner = self.scheduler();
        let ctx = RunContext::leaf(stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = runner.execute(&config(&runner, pairs), &ctx).await;
        (telemetry.outcome, produced)
    }

    async fn schedule_target(&self, target: ActionId, pairs: &[(&str, Variant)]) -> ScheduledRun {
        self.schedule_target_with(&ArgStack::new(), target, pairs)
            .await
    }

    async fn schedule_target_with(
        &self,
        stack: &ArgStack,
        target: ActionId,
        pairs: &[(&str, Variant)],
    ) -> ScheduledRun {
        let mut pairs = pairs.to_vec();
        pairs.push(("action_id", text(&target.to_string())));
        let (outcome, produced) = self.schedule(stack, &pairs).await;
        assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
        let id = produced
            .and_then(|stack| stack.get("schedule.id").and_then(Variant::as_int))
            .expect("the step reports the scheduled run id");
        self.pending_run(id).await
    }

    async fn pending(&self) -> Vec<ScheduledRun> {
        self.dp.scheduled_run_repo().list_pending().await.unwrap()
    }

    async fn pending_run(&self, id: i64) -> ScheduledRun {
        self.pending()
            .await
            .into_iter()
            .find(|run| run.id.get() == id)
            .expect("the reported run is pending")
    }

    async fn cancel(&self, stack: &ArgStack, key: &str) -> (SubActionOutcome, Option<Variant>) {
        let runner = self.canceller();
        let ctx = RunContext::leaf(stack, 0, EventId::new(), &NullPublisher);
        let (telemetry, produced) = runner
            .execute(&config(&runner, &[("key", text(key))]), &ctx)
            .await;
        let cancelled = produced.and_then(|stack| stack.get("schedule.cancelled").cloned());
        (telemetry.outcome, cancelled)
    }

    async fn next_run(&mut self) -> ExecutionContext {
        timeout(DEADLINE, self.runs.recv())
            .await
            .expect("no run was recorded")
            .expect("the run history closed")
    }
}

fn delay(amount: i64, unit: &str) -> [(&'static str, Variant); 2] {
    [
        ("delay_amount", Variant::Int(amount)),
        ("delay_unit", text(unit)),
    ]
}

fn due_at(value: Variant) -> [(&'static str, Variant); 2] {
    [("use_due_at", Variant::Bool(true)), ("due_at", value)]
}

fn rfc3339(at: OffsetDateTime) -> String {
    at.format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

#[tokio::test]
async fn schedule_step_places_the_run_after_the_delay_in_each_unit() {
    let harness = Harness::new().await;
    let target = harness.target().await;

    for (amount, unit, expected) in [
        (1, "minutes", MINUTE),
        (3, "hours", 3 * HOUR),
        (2, "days", 2 * DAY),
    ] {
        let run = harness.schedule_target(target, &delay(amount, unit)).await;
        assert_eq!(run.spec.due_at, base() + expected, "{amount} {unit}");
    }
}

#[tokio::test]
async fn schedule_step_reports_the_due_time_and_id_of_the_placed_run() {
    let harness = Harness::new().await;
    let target = harness.target().await;

    let (_, produced) = harness
        .schedule(
            &ArgStack::new(),
            &[
                ("action_id", text(&target.to_string())),
                ("delay_amount", Variant::Int(5)),
            ],
        )
        .await;

    let produced = produced.expect("the step produced its outputs");
    let pending = harness.pending().await;
    assert_eq!(
        (
            produced.get("schedule.due_at").cloned(),
            produced.get("schedule.id").cloned()
        ),
        (
            Some(Variant::Datetime(base() + 5 * MINUTE)),
            Some(Variant::Int(pending[0].id.get()))
        )
    );
}

#[tokio::test]
async fn schedule_step_writes_its_outputs_under_custom_variable_names() {
    let harness = Harness::new().await;
    let target = harness.target().await;

    let (_, produced) = harness
        .schedule(
            &ArgStack::new(),
            &[
                ("action_id", text(&target.to_string())),
                ("due_at_into_var", text("%reminder_at%")),
                ("id_into_var", text("reminder_id")),
            ],
        )
        .await;

    let produced = produced.expect("the step produced its outputs");
    let names = [
        "reminder_at",
        "reminder_id",
        "schedule.due_at",
        "schedule.id",
    ]
    .map(|name| produced.get(name).is_some());
    assert_eq!(names, [true, true, false, false]);
}

#[tokio::test]
async fn schedule_step_refuses_delays_outside_one_minute_to_one_year() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let too_short = ScheduleError::DelayTooShort.to_string();
    let too_long = ScheduleError::DelayTooLong.to_string();

    for (amount, unit, expected) in [
        (0, "minutes", Some(too_short.as_str())),
        (-5, "days", Some(too_short.as_str())),
        (1, "minutes", None),
        (365, "days", None),
        (366, "days", Some(too_long.as_str())),
        (i64::MAX, "days", Some(too_long.as_str())),
    ] {
        let mut pairs = delay(amount, unit).to_vec();
        pairs.push(("action_id", text(&target.to_string())));
        let (outcome, _) = harness.schedule(&ArgStack::new(), &pairs).await;
        assert_eq!(failure(&outcome), expected, "{amount} {unit}");
    }
}

#[tokio::test]
async fn schedule_step_resolves_the_due_time_from_each_accepted_input() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let due = base() + HOUR;
    let unix = due.unix_timestamp();

    for (stack, value) in [
        (ArgStack::new(), Variant::Datetime(due)),
        (ArgStack::new(), text(&rfc3339(due))),
        (ArgStack::new(), text(&unix.to_string())),
        (
            ArgStack::new().set("deadline".to_owned(), Variant::Datetime(due)),
            text("%deadline%"),
        ),
        (
            ArgStack::new().set("deadline".to_owned(), text(&rfc3339(due))),
            text("%deadline%"),
        ),
        (
            ArgStack::new().set("deadline".to_owned(), Variant::Int(unix)),
            text("%deadline%"),
        ),
    ] {
        let run = harness
            .schedule_target_with(&stack, target, &due_at(value.clone()))
            .await;
        assert_eq!(run.spec.due_at, due, "{value:?}");
    }
}

#[tokio::test]
async fn schedule_step_fails_on_a_due_time_it_cannot_read() {
    let harness = Harness::new().await;
    let target = harness.target().await;

    for value in ["", "next tuesday", "%missing%"] {
        let mut pairs = due_at(text(value)).to_vec();
        pairs.push(("action_id", text(&target.to_string())));
        let (outcome, produced) = harness.schedule(&ArgStack::new(), &pairs).await;
        assert!(
            failure(&outcome).is_some_and(|reason| reason.starts_with("due date-time")),
            "{value:?}: {outcome:?}"
        );
        assert!(produced.is_none(), "{value:?}");
    }
}

#[tokio::test]
async fn schedule_step_refuses_a_due_time_that_is_not_ahead_of_now() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let too_short = ScheduleError::DelayTooShort.to_string();

    for due in [base() - DAY, base()] {
        let mut pairs = due_at(Variant::Datetime(due)).to_vec();
        pairs.push(("action_id", text(&target.to_string())));
        let (outcome, _) = harness.schedule(&ArgStack::new(), &pairs).await;
        assert_eq!(failure(&outcome), Some(too_short.as_str()), "{due}");
    }
}

#[tokio::test]
async fn schedule_step_without_a_known_action_fails_without_placing_a_run() {
    let harness = Harness::new().await;
    let unknown = ScheduleError::UnknownAction.to_string();

    for (action_id, expected) in [
        (String::new(), "no valid action is selected"),
        ("not-an-action".to_owned(), "no valid action is selected"),
        (ActionId::new().to_string(), unknown.as_str()),
    ] {
        let (outcome, _) = harness
            .schedule(&ArgStack::new(), &[("action_id", text(&action_id))])
            .await;
        assert_eq!(failure(&outcome), Some(expected), "{action_id:?}");
    }
    assert!(harness.pending().await.is_empty());
}

#[tokio::test]
async fn schedule_step_interpolates_the_key_and_a_repeat_replaces_the_pending_run() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let stack = ArgStack::new().set("user".to_owned(), text("alice"));
    let keyed = [("key", text("vip:%user%"))];

    harness.schedule_target_with(&stack, target, &keyed).await;
    let latest = harness.schedule_target_with(&stack, target, &keyed).await;

    let pending: Vec<_> = harness
        .pending()
        .await
        .into_iter()
        .map(|run| (run.id, run.spec.key))
        .collect();
    assert_eq!(pending, vec![(latest.id, Some("vip:alice".to_owned()))]);
}

#[tokio::test]
async fn schedule_step_with_a_blank_key_stores_no_key_and_replaces_nothing() {
    let harness = Harness::new().await;
    let target = harness.target().await;

    for key in ["", "   "] {
        harness.schedule_target(target, &[("key", text(key))]).await;
    }

    let keys: Vec<_> = harness
        .pending()
        .await
        .into_iter()
        .map(|run| run.spec.key)
        .collect();
    assert_eq!(keys, vec![None, None]);
}

#[tokio::test]
async fn schedule_step_captures_the_current_variables_only_when_inheriting() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let stack = ArgStack::new().set("user".to_owned(), text("alice"));

    for (inherit, expected) in [
        (None, stack.snapshot()),
        (Some(true), stack.snapshot()),
        (Some(false), BTreeMap::new()),
    ] {
        let pairs: Vec<_> = inherit
            .map(|on| ("inherit_args", Variant::Bool(on)))
            .into_iter()
            .collect();
        let run = harness.schedule_target_with(&stack, target, &pairs).await;
        assert_eq!(run.spec.args, expected, "{inherit:?}");
    }
}

#[tokio::test]
async fn schedule_step_refuses_oversized_variables_unless_it_does_not_inherit_them() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let stack = ArgStack::new().set(
        "blob".to_owned(),
        text(&"x".repeat(MAX_SCHEDULED_ARGS_BYTES)),
    );

    for (inherit, refused) in [(true, true), (false, false)] {
        let (outcome, _) = harness
            .schedule(
                &stack,
                &[
                    ("action_id", text(&target.to_string())),
                    ("inherit_args", Variant::Bool(inherit)),
                ],
            )
            .await;
        assert_eq!(
            failure(&outcome)
                .is_some_and(|reason| reason.starts_with("the captured variables take")),
            refused,
            "inherit {inherit}: {outcome:?}"
        );
    }
}

#[tokio::test]
async fn schedule_step_maps_the_skip_toggle_to_the_stored_policy() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let minimum = i64::try_from(MIN_LATE_TOLERANCE.as_secs() / 60).unwrap();

    for (pairs, expected) in [
        (vec![], MissedRunPolicy::RunLateOnce),
        (
            vec![
                ("skip_if_late", Variant::Bool(false)),
                ("late_tolerance_minutes", Variant::Int(90)),
            ],
            MissedRunPolicy::RunLateOnce,
        ),
        (
            vec![
                ("missed_policy", text("skip if late")),
                ("late_tolerance_minutes", Variant::Int(90)),
            ],
            MissedRunPolicy::RunLateOnce,
        ),
        (
            vec![
                ("skip_if_late", Variant::Bool(true)),
                ("late_tolerance_minutes", Variant::Int(minimum)),
            ],
            MissedRunPolicy::SkipIfLateBy(MIN_LATE_TOLERANCE),
        ),
        (
            vec![
                ("skip_if_late", Variant::Bool(true)),
                ("late_tolerance_minutes", Variant::Int(90)),
            ],
            MissedRunPolicy::SkipIfLateBy(90 * MINUTE),
        ),
    ] {
        let run = harness.schedule_target(target, &pairs).await;
        assert_eq!(run.spec.missed_run_policy, expected, "{pairs:?}");
    }
}

#[tokio::test]
async fn schedule_step_refuses_a_skip_tolerance_below_the_minimum() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let out_of_range = ScheduleError::LateToleranceOutOfRange.to_string();
    let below = i64::try_from(MIN_LATE_TOLERANCE.as_secs() / 60).unwrap() - 1;

    for minutes in [below, 0, -5] {
        let (outcome, _) = harness
            .schedule(
                &ArgStack::new(),
                &[
                    ("action_id", text(&target.to_string())),
                    ("skip_if_late", Variant::Bool(true)),
                    ("late_tolerance_minutes", Variant::Int(minutes)),
                ],
            )
            .await;
        assert_eq!(failure(&outcome), Some(out_of_range.as_str()), "{minutes}");
    }
}

#[tokio::test]
async fn cancel_step_reports_whether_a_pending_run_held_the_exact_key() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let stack = ArgStack::new().set("user".to_owned(), text("alice"));
    for key in ["vip:alice", "vip:bob"] {
        harness.schedule_target(target, &[("key", text(key))]).await;
    }

    let mut seen = Vec::new();
    for key in [
        "vip:%user%",
        "vip:%user%",
        "VIP:bob",
        "vip:carol",
        "",
        "  ",
        "  vip:bob  ",
    ] {
        let (_, cancelled) = harness.cancel(&stack, key).await;
        seen.push((key, cancelled));
    }

    let expected: Vec<_> = [
        ("vip:%user%", true),
        ("vip:%user%", false),
        ("VIP:bob", false),
        ("vip:carol", false),
        ("", false),
        ("  ", false),
        ("  vip:bob  ", true),
    ]
    .into_iter()
    .map(|(key, cancelled)| (key, Some(Variant::Bool(cancelled))))
    .collect();
    assert_eq!(seen, expected);
}

#[tokio::test]
async fn schedule_and_cancel_steps_fail_while_the_scheduler_is_not_running() {
    let target = ActionId::new().to_string();
    let schedule = CoreActionScheduleRunner::new(ScheduledRunsCell::new());
    let cancel = CoreActionCancelScheduledRunner::new(ScheduledRunsCell::new());
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    let stopped = ScheduleError::SchedulerStopped.to_string();

    for (runner, pairs) in [
        (
            &schedule as &dyn SubActionRunner,
            vec![("action_id", text(&target))],
        ),
        (&cancel as &dyn SubActionRunner, vec![("key", text("k"))]),
    ] {
        let (telemetry, produced) = runner.execute(&config(runner, &pairs), &ctx).await;
        assert_eq!(
            (failure(&telemetry.outcome), produced.is_none()),
            (Some(stopped.as_str()), true),
            "{}",
            runner.id()
        );
    }
}

type Origin = (Option<ActionId>, Option<String>, Option<EventId>);

fn origins(runs: &[ScheduledRun]) -> Vec<Origin> {
    runs.iter()
        .map(|run| {
            (
                run.spec.scheduled_by_action,
                run.spec.scheduled_by_run.clone(),
                run.spec.trigger_event_id,
            )
        })
        .collect()
}

fn schedule_step(target: ActionId) -> SubActionStep {
    let mut config = SubActionConfig::new();
    config.insert("action_id".to_owned(), text(&target.to_string()));
    step(SCHEDULE_ACTION_KIND_ID, config)
}

async fn run_through_engine(harness: &mut Harness, action: &Action) -> (EventId, EventId) {
    let mut events = harness.bus.subscribe();
    let trigger = EventId::new();
    harness
        .engine
        .dispatch(ExecutionRequest {
            action_id: action.id,
            trigger_event_id: trigger,
            trigger_kind: None,
            initial_args: ArgStack::new(),
        })
        .await
        .unwrap();
    let context = harness.next_run().await;
    assert!(
        context
            .telemetry
            .iter()
            .all(|step| !matches!(step.outcome, SubActionOutcome::Failed(_))),
        "{:?}",
        context.telemetry
    );
    let mut start = None;
    while let Ok(Some(event)) = events.try_recv() {
        if event.kind == "action.start"
            && event.payload["action_id"].as_str() == Some(action.id.to_string().as_str())
        {
            start = Some(event.id);
        }
    }
    (trigger, start.expect("the run published action.start"))
}

#[tokio::test]
async fn a_scheduling_run_records_its_action_start_and_trigger_as_the_origin() {
    for concurrent in [false, true] {
        let mut harness = Harness::new().await;
        let target = harness.target().await;
        let scheduling = harness
            .action(concurrent, vec![schedule_step(target)])
            .await;

        let (trigger, start) = run_through_engine(&mut harness, &scheduling).await;

        let pending = harness.pending().await;
        assert_eq!(
            origins(&pending),
            vec![(Some(scheduling.id), Some(start.to_string()), Some(trigger))],
            "concurrent {concurrent}"
        );
    }
}

#[tokio::test]
async fn a_schedule_step_inside_a_run_action_child_reports_the_outer_run() {
    let mut harness = Harness::new().await;
    let target = harness.target().await;
    let child = harness.action(false, vec![schedule_step(target)]).await;
    let mut run_child = SubActionConfig::new();
    run_child.insert("action_id".to_owned(), text(&child.id.to_string()));
    let outer = harness
        .action(false, vec![step("core.action.run", run_child)])
        .await;

    let (trigger, start) = run_through_engine(&mut harness, &outer).await;

    let pending = harness.pending().await;
    assert_eq!(
        origins(&pending),
        vec![(Some(outer.id), Some(start.to_string()), Some(trigger))]
    );
}

#[tokio::test]
async fn a_quick_action_schedule_has_no_scheduling_run_and_is_caused_by_its_step_event() {
    let harness = Harness::new().await;
    let target = harness.target().await;
    let mut events = harness.bus.subscribe();

    let outcome = harness
        .engine
        .execute_quick_action(
            schedule_step(target),
            "core".to_owned(),
            "Schedule".to_owned(),
            None,
        )
        .await
        .unwrap()
        .outcome()
        .await
        .unwrap();
    assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");

    let mut step_event = None;
    while let Ok(Some(event)) = events.try_recv() {
        if event.kind == "subaction.run" {
            step_event = Some(event.id);
        }
    }
    let pending = harness.pending().await;
    assert_eq!(origins(&pending), vec![(None, None, step_event)]);
}

#[test]
fn both_steps_are_registered_under_their_kind_ids() {
    let mut registry = SubActionRegistry::new();
    forge_runtime::register_scheduled_run_sub_actions(&mut registry, ScheduledRunsCell::new())
        .unwrap();

    let present =
        [SCHEDULE_ACTION_KIND_ID, CANCEL_SCHEDULED_KIND_ID].map(|id| registry.get(id).is_some());
    assert_eq!(present, [true, true]);
}
