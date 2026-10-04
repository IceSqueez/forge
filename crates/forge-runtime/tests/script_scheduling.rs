#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRegistry, SubActionRunner};
use forge_runtime::scheduled_runs::{MAX_SCHEDULE_DELAY, MIN_LATE_TOLERANCE};
use forge_runtime::sub_action_runners::ScriptRunInlineRunner;
use forge_runtime::{
    ActionCancelRegistry, Catalog, CatchUpSettle, EventBus, NullEventLogRepo, QueueScheduler,
    ScheduleError, ScheduledRunsCell, ScheduledRunsParts, SchedulingContext, ScriptRegistry,
    ScriptScheduling, WallClock, spawn_action_engine, spawn_scheduled_runs,
};
use forge_script::{
    ActionScheduler, ScriptScheduleDue, ScriptScheduleError, ScriptSchedulePlacement,
    ScriptScheduleRequest,
};
use forge_storage::action::MockActionRepo;
use forge_storage::history::MockHistoryRepo;
use forge_storage::{
    DataProvider, GlobalsRepo, MissedRunPolicy, ScheduledRun, SettingsRepo, StorageError,
};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionMode, Queue, QueueId, SubActionOutcome, Variant,
};
use tempfile::TempDir;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

const BASE_UNIX_SECS: i64 = 1_791_115_200;
const HOUR: Duration = Duration::from_secs(60 * 60);
const RAID_KEY: &str = "raid-thanks";

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

fn request(action: &str, due: ScriptScheduleDue) -> ScriptScheduleRequest {
    ScriptScheduleRequest {
        action_id_or_name: action.to_owned(),
        due,
        key: None,
        inherit_args: true,
        skip_if_late_minutes: None,
    }
}

fn in_an_hour(action: &str) -> ScriptScheduleRequest {
    request(
        action,
        ScriptScheduleDue::AfterSeconds(i64::try_from(HOUR.as_secs()).unwrap()),
    )
}

fn rejected(error: ScheduleError) -> ScriptScheduleError {
    ScriptScheduleError::Rejected(error.to_string())
}

struct Harness {
    backend: Arc<SqliteBackend>,
    _media: TempDir,
    queue_id: QueueId,
    cell: ScheduledRunsCell,
}

impl Harness {
    async fn new() -> Self {
        let media = tempfile::tempdir().unwrap();
        let backend = Arc::new(
            SqliteBackend::open_for_test(":memory:", [0xcd; 32], media.path().join("media"), None)
                .await
                .unwrap(),
        );
        let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;
        let queue = Queue {
            id: QueueId::new(),
            name: "scheduled".to_owned(),
            description: String::new(),
            concurrency: 8,
        };
        dp.queue_repo().save(&queue).await.unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let catalog = Catalog::from_provider(dp.as_ref());
        let mut history = MockHistoryRepo::new();
        history.expect_save_batch().returning(|_| Ok(()));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&catalog),
            dp.action_repo(),
            Arc::new(history),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        );
        let queues = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue.clone()]);
        let cell = ScheduledRunsCell::new();
        cell.set(spawn_scheduled_runs(ScheduledRunsParts {
            repo: dp.scheduled_run_repo(),
            revision: dp.scheduled_run_revision(),
            catalog,
            actions: dp.action_repo(),
            queues,
            bus,
            clock: Arc::new(FixedClock),
            catch_up: CatchUpSettle::immediately(),
        }));
        Self {
            backend,
            _media: media,
            queue_id: queue.id,
            cell,
        }
    }

    async fn action_named(&self, name: &str) -> ActionId {
        let action = Action {
            id: ActionId::new(),
            name: name.to_owned(),
            group: None,
            queue_id: self.queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: Vec::new(),
        };
        self.backend.action_repo().save(&action).await.unwrap();
        action.id
    }

    fn scheduling(&self) -> ScriptScheduling {
        ScriptScheduling::new(self.cell.clone(), self.backend.action_repo())
    }

    fn scheduler(&self, args: ArgStack) -> Arc<dyn ActionScheduler> {
        self.scheduling()
            .for_context(SchedulingContext::outside_any_run(args))
    }

    async fn schedule(
        &self,
        request: ScriptScheduleRequest,
    ) -> Result<ScriptSchedulePlacement, ScriptScheduleError> {
        self.scheduler(ArgStack::new()).schedule(request).await
    }

    async fn pending(&self) -> Vec<ScheduledRun> {
        self.backend
            .scheduled_run_repo()
            .list_pending()
            .await
            .unwrap()
    }

    async fn placed(&self, placement: ScriptSchedulePlacement) -> ScheduledRun {
        self.pending()
            .await
            .into_iter()
            .find(|run| run.id.get() == placement.id)
            .expect("the reported run is pending")
    }
}

#[tokio::test]
async fn an_action_is_found_by_its_id_or_its_exact_name_ignoring_surrounding_spaces() {
    let harness = Harness::new().await;
    let greet = harness.action_named("Greet").await;
    harness.action_named("Greeting").await;

    for wanted in [
        greet.to_string(),
        format!("  {greet}\t"),
        "Greet".to_owned(),
        " Greet ".to_owned(),
    ] {
        let placement = harness.schedule(in_an_hour(&wanted)).await.unwrap();

        assert_eq!(
            harness.placed(placement).await.spec.target_action_id,
            greet,
            "{wanted:?}"
        );
    }
}

#[tokio::test]
async fn a_disabled_action_is_still_found_by_name() {
    let harness = Harness::new().await;
    let greet = harness.action_named("Greet").await;
    harness
        .backend
        .action_repo()
        .set_enabled(greet, false)
        .await
        .unwrap();

    let placement = harness.schedule(in_an_hour("Greet")).await.unwrap();

    assert_eq!(harness.placed(placement).await.spec.target_action_id, greet);
}

#[tokio::test]
async fn a_name_matching_no_live_action_exactly_is_unknown_and_places_nothing() {
    let harness = Harness::new().await;
    harness.action_named("Greet").await;
    let archived = harness.action_named("Farewell").await;
    harness
        .backend
        .action_repo()
        .archive(archived)
        .await
        .unwrap();

    for wanted in ["greet", "Gree", "Greet!", "Farewell", ""] {
        let result = harness.schedule(in_an_hour(wanted)).await;

        assert_eq!(
            result,
            Err(ScriptScheduleError::UnknownAction(wanted.to_owned())),
            "{wanted:?}"
        );
    }
    assert!(harness.pending().await.is_empty());
}

#[tokio::test]
async fn an_id_of_no_live_action_is_refused_by_the_scheduler() {
    let harness = Harness::new().await;
    let archived = harness.action_named("Farewell").await;
    harness
        .backend
        .action_repo()
        .archive(archived)
        .await
        .unwrap();

    for id in [ActionId::new(), archived] {
        let result = harness.schedule(in_an_hour(&id.to_string())).await;

        assert_eq!(result, Err(rejected(ScheduleError::UnknownAction)), "{id}");
    }
}

#[tokio::test]
async fn a_name_shared_by_several_actions_is_ambiguous_with_their_count() {
    let harness = Harness::new().await;
    for _ in 0..3 {
        harness.action_named("Dup").await;
    }

    let result = harness.schedule(in_an_hour(" Dup ")).await;

    assert_eq!(
        result,
        Err(ScriptScheduleError::AmbiguousAction {
            name: "Dup".to_owned(),
            count: 3,
        })
    );
    assert!(harness.pending().await.is_empty());
}

#[tokio::test]
async fn a_failing_action_list_makes_name_lookup_unavailable() {
    let harness = Harness::new().await;
    let mut actions = MockActionRepo::new();
    actions
        .expect_list()
        .returning(|| Err(StorageError::NotReady));
    let scheduler = ScriptScheduling::new(harness.cell.clone(), Arc::new(actions))
        .for_context(SchedulingContext::outside_any_run(ArgStack::new()));

    let result = scheduler.schedule(in_an_hour("Greet")).await;

    assert_eq!(
        result,
        Err(ScriptScheduleError::ActionsUnavailable(
            StorageError::NotReady.to_string()
        ))
    );
}

#[tokio::test]
async fn scheduling_and_cancelling_fail_as_stopped_before_the_scheduler_is_set() {
    let harness = Harness::new().await;
    let greet = harness.action_named("Greet").await;
    let scheduler = ScriptScheduling::new(ScheduledRunsCell::new(), harness.backend.action_repo())
        .for_context(SchedulingContext::outside_any_run(ArgStack::new()));
    let stopped = rejected(ScheduleError::SchedulerStopped);

    let scheduled = scheduler.schedule(in_an_hour(&greet.to_string())).await;
    let cancelled = scheduler.cancel_by_key(RAID_KEY).await;

    assert_eq!((scheduled, cancelled), (Err(stopped.clone()), Err(stopped)));
}

#[tokio::test]
async fn a_delay_in_seconds_is_bounded_and_negative_delays_are_too_short() {
    let harness = Harness::new().await;
    harness.action_named("Greet").await;
    let too_short = Err(rejected(ScheduleError::DelayTooShort));
    let too_long = Err(rejected(ScheduleError::DelayTooLong));
    let max = i64::try_from(MAX_SCHEDULE_DELAY.as_secs()).unwrap();

    for (seconds, expected) in [
        (i64::MIN, too_short.clone()),
        (-1, too_short.clone()),
        (0, too_short.clone()),
        (59, too_short),
        (60, Ok(base() + Duration::from_secs(60))),
        (max, Ok(base() + MAX_SCHEDULE_DELAY)),
        (max + 1, too_long.clone()),
        (i64::MAX, too_long),
    ] {
        let result = harness
            .schedule(request("Greet", ScriptScheduleDue::AfterSeconds(seconds)))
            .await
            .map(|placement| placement.due_at);

        assert_eq!(result, expected, "{seconds} s");
    }
}

#[tokio::test]
async fn a_due_time_is_read_from_rfc3339_text_unix_seconds_or_unix_text() {
    let harness = Harness::new().await;
    harness.action_named("Greet").await;
    let due = base() + HOUR;
    let unix = due.unix_timestamp();
    let offset = time::UtcOffset::from_hms(3, 0, 0).unwrap();

    for when in [
        text(&due.format(&Rfc3339).unwrap()),
        text(&due.to_offset(offset).format(&Rfc3339).unwrap()),
        Variant::Int(unix),
        text(&unix.to_string()),
    ] {
        let placement = harness
            .schedule(request("Greet", ScriptScheduleDue::At(when.clone())))
            .await
            .unwrap();

        assert_eq!(
            (
                placement.due_at,
                harness.placed(placement).await.spec.due_at
            ),
            (due, due),
            "{when:?}"
        );
    }
}

#[tokio::test]
async fn an_unreadable_due_time_is_refused_naming_the_due_date_time() {
    let harness = Harness::new().await;
    harness.action_named("Greet").await;

    for when in [text("next tuesday"), text(""), Variant::Int(i64::MAX)] {
        let result = harness
            .schedule(request("Greet", ScriptScheduleDue::At(when.clone())))
            .await;

        assert!(
            matches!(&result, Err(ScriptScheduleError::Rejected(reason)) if reason.starts_with("due date-time:")),
            "{when:?}: {result:?}"
        );
    }
    assert!(harness.pending().await.is_empty());
}

#[tokio::test]
async fn a_due_time_not_ahead_of_now_is_too_short() {
    let harness = Harness::new().await;
    harness.action_named("Greet").await;

    for due in [base() - HOUR, base()] {
        let result = harness
            .schedule(request(
                "Greet",
                ScriptScheduleDue::At(Variant::Int(due.unix_timestamp())),
            ))
            .await;

        assert_eq!(result, Err(rejected(ScheduleError::DelayTooShort)), "{due}");
    }
}

#[tokio::test]
async fn skip_if_late_minutes_maps_to_the_stored_policy_within_its_bounds() {
    let harness = Harness::new().await;
    harness.action_named("Greet").await;
    let min_minutes = i64::try_from(MIN_LATE_TOLERANCE.as_secs() / 60).unwrap();
    let out_of_range = Err(rejected(ScheduleError::LateToleranceOutOfRange));

    for (minutes, expected) in [
        (None, Ok(MissedRunPolicy::RunLateOnce)),
        (
            Some(min_minutes),
            Ok(MissedRunPolicy::SkipIfLateBy(MIN_LATE_TOLERANCE)),
        ),
        (Some(min_minutes - 1), out_of_range.clone()),
        (Some(-1), out_of_range.clone()),
        (Some(i64::MAX), out_of_range),
    ] {
        let placed = harness
            .schedule(ScriptScheduleRequest {
                skip_if_late_minutes: minutes,
                ..in_an_hour("Greet")
            })
            .await;
        let policy = match placed {
            Ok(placement) => Ok(harness.placed(placement).await.spec.missed_run_policy),
            Err(e) => Err(e),
        };

        assert_eq!(policy, expected, "{minutes:?}");
    }
}

#[tokio::test]
async fn a_run_scheduled_outside_any_run_has_no_origin_and_inherits_only_when_asked() {
    let harness = Harness::new().await;
    harness.action_named("Greet").await;
    let args = ArgStack::new().set("user".to_owned(), text("ice"));

    for (inherit_args, expected_args) in [
        (true, BTreeMap::from([("user".to_owned(), text("ice"))])),
        (false, BTreeMap::new()),
    ] {
        let placement = harness
            .scheduler(args.clone())
            .schedule(ScriptScheduleRequest {
                inherit_args,
                ..in_an_hour("Greet")
            })
            .await
            .unwrap();
        let spec = harness.placed(placement).await.spec;

        assert_eq!(
            (
                spec.args,
                spec.scheduled_by_action,
                spec.scheduled_by_run,
                spec.trigger_event_id
            ),
            (expected_args, None, None, None),
            "inherit_args {inherit_args}"
        );
    }
}

#[tokio::test]
async fn cancel_by_key_reports_whether_a_pending_run_held_the_key() {
    let harness = Harness::new().await;
    harness.action_named("Greet").await;
    harness
        .schedule(ScriptScheduleRequest {
            key: Some(RAID_KEY.to_owned()),
            ..in_an_hour("Greet")
        })
        .await
        .unwrap();
    let scheduler = harness.scheduler(ArgStack::new());

    let first = scheduler.cancel_by_key(RAID_KEY).await;
    let second = scheduler.cancel_by_key(RAID_KEY).await;

    assert_eq!((first, second), (Ok(true), Ok(false)));
}

#[tokio::test]
async fn an_inline_script_step_schedules_with_its_run_as_the_cause_and_its_variables() {
    let harness = Harness::new().await;
    let greet = harness.action_named("Greet").await;
    let mut registry = ScriptRegistry::new();
    registry.set_scheduling(harness.scheduling());
    let runner = ScriptRunInlineRunner::new(
        Arc::new(registry),
        Arc::clone(&harness.backend) as Arc<dyn GlobalsRepo>,
        Arc::new(NullPublisher),
        Arc::clone(&harness.backend) as Arc<dyn SettingsRepo>,
    );
    let mut config = runner.default_config();
    config.insert(
        "body".to_owned(),
        text(r#"forge::schedule::after("Greet", 3600, #{key: "raid-thanks"});"#),
    );
    let stack = ArgStack::new().set("user".to_owned(), text("ice"));
    let cause = EventId::new();
    let ctx = RunContext::leaf(&stack, 0, cause, &NullPublisher);

    let (telemetry, _) = runner.execute(&config, &ctx).await;

    assert!(
        matches!(telemetry.outcome, SubActionOutcome::Success),
        "{:?}",
        telemetry.outcome
    );
    let spec = harness.pending().await.remove(0).spec;
    assert_eq!(
        (
            spec.target_action_id,
            spec.key,
            spec.trigger_event_id,
            spec.args
        ),
        (
            greet,
            Some(RAID_KEY.to_owned()),
            Some(cause),
            BTreeMap::from([("user".to_owned(), text("ice"))])
        )
    );
}
