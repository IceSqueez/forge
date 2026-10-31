#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use common::stub_provider::StubProvider;
use forge_events::EventPublisher;
use forge_registry::RunContext;
use forge_runtime::sub_action_runners::ScriptRunInlineRunner;
use forge_runtime::{
    CatchUpSettle, EventBus, FrozenRuntime, NullEventLogRepo, QueueAdmission, QueueMode,
    RuntimeWiring, ScheduleDue, ScheduleError, ScheduleRequest, ScheduledRunsHandle,
    SchedulerRequest, SchedulingContext, ScriptRegistry, ScriptRegistryParts,
    register_core_triggers,
};
use forge_script::{ScriptSpeakError, SpeakRequester};
use forge_storage::queue::MockQueueRepo;
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{DataProvider, GlobalsRepo, MissedRunPolicy, SettingsRepo, StorageError};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ActionId, ArgStack, EventId, LatestScope, LatestValue, LatestValueReader, Queue,
    QueueId, SubActionConfig, SubActionOutcome, SubActionStep, TriggerInstanceId, Variant,
};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::time::timeout;

const HANG_GUARD: Duration = Duration::from_secs(30);
const INLINE_BODY_KEY: &str = "body";
const QUEUE_PAUSE_KIND_ID: &str = "core.queue.pause";
const DEFAULTS_STORED: &str = "defaults stored";
const QUEUES_READ: &str = "queues read";
const CATALOG_BUILT: &str = "catalog built";

async fn sqlite() -> (Arc<SqliteBackend>, TempDir) {
    let (backend, media) = common::sqlite_backend().await;
    forge_storage::write_setting(&backend, &forge_script::settings::TIMEOUT_MS, &10_000)
        .await
        .unwrap();
    (Arc::new(backend), media)
}

fn bus() -> Arc<EventBus> {
    EventBus::new(Arc::new(NullEventLogRepo))
}

fn queue(name: &str) -> Queue {
    Queue {
        id: QueueId::new(),
        name: name.to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::new(8).unwrap(),
    }
}

fn action(queue_id: QueueId, sub_actions: Vec<SubActionStep>) -> Action {
    common::sequential_action(ActionId::new(), "wired", queue_id, sub_actions)
}

fn pause_step(queue_id: QueueId) -> SubActionStep {
    SubActionStep {
        kind_id: QUEUE_PAUSE_KIND_ID.to_owned(),
        config: SubActionConfig::from([(
            "queue_id".to_owned(),
            Variant::String(queue_id.to_string()),
        )]),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn schedule_of(target_action_id: ActionId) -> ScheduleRequest {
    ScheduleRequest {
        target_action_id,
        due: ScheduleDue::After(Duration::from_secs(60 * 60)),
        key: None,
        missed_run_policy: MissedRunPolicy::RunLateOnce,
        args: BTreeMap::new(),
        scheduled_by_action: None,
        scheduled_by_run: None,
        trigger_event_id: None,
        cause: None,
    }
}

async fn runtime_on(provider: StubProvider) -> FrozenRuntime {
    let mut wiring = RuntimeWiring::new(common::wiring_parts(Arc::new(provider), bus()).await);
    register_core_triggers(wiring.triggers_mut()).unwrap();
    wiring.freeze().await
}

fn journal_of(journal: &mpsc::UnboundedSender<&'static str>, entry: &'static str) {
    journal.send(entry).unwrap();
}

async fn distinct_until_catalog(
    journal: &mut mpsc::UnboundedReceiver<&'static str>,
) -> Vec<&'static str> {
    let mut order = Vec::new();
    while order.last() != Some(&CATALOG_BUILT) {
        let entry = timeout(HANG_GUARD, journal.recv())
            .await
            .expect("the evaluator never built its catalog")
            .unwrap();
        if order.last() != Some(&entry) {
            order.push(entry);
        }
    }
    order
}

#[tokio::test]
async fn freeze_stores_defaults_then_reads_queues_before_the_evaluator_builds_its_catalog() {
    let (journal, mut order) = mpsc::unbounded_channel();
    let mut provider = StubProvider::new();
    let defaults = journal.clone();
    provider
        .trigger_instance()
        .expect_upsert_default()
        .returning(move |_, _| {
            journal_of(&defaults, DEFAULTS_STORED);
            Ok(TriggerInstanceId::new())
        });
    let queues = journal.clone();
    provider.queue().expect_list().returning(move || {
        journal_of(&queues, QUEUES_READ);
        Ok(Vec::new())
    });
    provider.action().expect_list().returning(move || {
        journal_of(&journal, CATALOG_BUILT);
        Ok(Vec::new())
    });
    let idle = StubProvider::idle();
    provider.latest_value_repo = idle.latest_value_repo;
    provider.media_repo = idle.media_repo;
    let bus = bus();
    let mut wiring =
        RuntimeWiring::new(common::wiring_parts(Arc::new(provider), Arc::clone(&bus)).await);
    register_core_triggers(wiring.triggers_mut()).unwrap();

    let _runtime = wiring.freeze().await;
    bus.publish(forge_events::Event::new(
        forge_events::EventSource::Server,
        "custom.anything",
        serde_json::json!({}),
    ));

    assert_eq!(
        distinct_until_catalog(&mut order).await,
        [DEFAULTS_STORED, QUEUES_READ, CATALOG_BUILT],
    );
}

#[tokio::test]
async fn a_clean_freeze_reports_both_stores_and_serves_the_stored_queue() {
    let (backend, _media) = sqlite().await;
    let stored = queue("stored");
    backend.queue_repo().save(&stored).await.unwrap();
    let provider = Arc::clone(&backend) as Arc<dyn DataProvider>;
    let mut wiring = RuntimeWiring::new(common::wiring_parts(provider, bus()).await);
    register_core_triggers(wiring.triggers_mut()).unwrap();

    let runtime = wiring.freeze().await;

    let states = runtime.scheduler.queue_states().await.unwrap();
    assert_eq!(
        (
            runtime.trigger_defaults_stored,
            runtime.queues_loaded,
            states.contains_key(&stored.id),
        ),
        (true, true, true),
    );
}

#[tokio::test]
async fn a_failed_queue_read_is_reported_and_the_scheduler_runs_with_no_queues() {
    let mut provider = StubProvider::idle();
    let mut queues = MockQueueRepo::new();
    queues
        .expect_list()
        .returning(|| Err(StorageError::NotReady));
    provider.queue_repo = Arc::new(queues);

    let runtime = runtime_on(provider).await;

    let states = runtime.scheduler.queue_states().await.unwrap();
    assert_eq!(
        (
            runtime.queues_loaded,
            runtime.trigger_defaults_stored,
            states.len()
        ),
        (false, true, 0),
    );
}

#[tokio::test]
async fn one_failed_default_is_reported_and_the_other_defaults_are_still_written() {
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&attempts);
    let mut provider = StubProvider::idle();
    let mut instances = MockTriggerInstanceRepo::new();
    instances.expect_upsert_default().returning(move |kind, _| {
        let mut seen = seen.lock().unwrap();
        seen.push(kind.to_owned());
        if seen.len() == 1 {
            return Err(StorageError::NotReady);
        }
        Ok(TriggerInstanceId::new())
    });
    provider.trigger_instance_repo = Arc::new(instances);

    let runtime = runtime_on(provider).await;

    assert_eq!(
        (
            runtime.trigger_defaults_stored,
            runtime.queues_loaded,
            attempts.lock().unwrap().len(),
        ),
        (false, true, runtime.trigger_registry.all().count()),
    );
}

#[tokio::test]
async fn registering_a_step_family_twice_is_refused() {
    let provider: Arc<dyn DataProvider> = Arc::new(StubProvider::idle());
    let mut wiring = common::with_every_core_step(RuntimeWiring::new(
        common::wiring_parts(provider, bus()).await,
    ));

    assert_eq!(
        (
            wiring.register_core_sub_actions().is_err(),
            wiring.register_scheduled_run_sub_actions().is_err(),
        ),
        (true, true),
    );
}

#[tokio::test]
async fn a_queue_pause_step_in_a_run_pauses_its_target_on_a_frozen_runtime() {
    let (backend, _media) = sqlite().await;
    let home = queue("home");
    let target = queue("target");
    backend.queue_repo().save(&home).await.unwrap();
    backend.queue_repo().save(&target).await.unwrap();
    let pausing = action(home.id, vec![pause_step(target.id)]);
    backend.action_repo().save(&pausing).await.unwrap();
    let bus = bus();
    let mut done = bus.subscribe();
    let provider = Arc::clone(&backend) as Arc<dyn DataProvider>;
    let runtime =
        common::frozen_runtime(common::wiring_parts(provider, Arc::clone(&bus)).await).await;

    runtime
        .scheduler
        .dispatch(SchedulerRequest {
            queue_id: home.id,
            action_id: pausing.id,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            initial_args: ArgStack::new(),
            admission: QueueAdmission::default(),
        })
        .await
        .unwrap();
    timeout(HANG_GUARD, async {
        while done.recv().await.unwrap().kind != "action.done" {}
    })
    .await
    .expect("the pausing run never finished");

    let states = runtime.scheduler.queue_states().await.unwrap();
    assert_eq!(states[&target.id].mode, QueueMode::PAUSED);
}

async fn parked_schedule(
    target: ActionId,
    runs: &ScheduledRunsHandle,
) -> tokio::task::JoinHandle<Result<(), String>> {
    let runs = runs.clone();
    tokio::spawn(async move {
        runs.schedule(schedule_of(target))
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

#[tokio::test]
async fn a_schedule_sent_before_the_worker_starts_completes_once_it_does() {
    let (backend, _media) = sqlite().await;
    let home = queue("home");
    backend.queue_repo().save(&home).await.unwrap();
    let target = action(home.id, Vec::new());
    backend.action_repo().save(&target).await.unwrap();
    let provider = Arc::clone(&backend) as Arc<dyn DataProvider>;
    let runtime = common::frozen_runtime(common::wiring_parts(provider, bus()).await).await;
    let pending = parked_schedule(target.id, &runtime.scheduled_runs).await;
    tokio::task::yield_now().await;
    let parked_before_start = !pending.is_finished();

    runtime
        .scheduled_runs_worker
        .start(CatchUpSettle::immediately());
    let placed = timeout(HANG_GUARD, pending).await.unwrap().unwrap();

    let stored = backend.scheduled_run_repo().list_pending().await.unwrap();
    assert_eq!(
        (parked_before_start, placed, stored.len()),
        (true, Ok(()), 1),
    );
}

#[tokio::test]
async fn a_schedule_waiting_on_a_worker_that_never_starts_ends_as_stopped() {
    let provider: Arc<dyn DataProvider> = Arc::new(StubProvider::idle());
    let runtime = common::frozen_runtime(common::wiring_parts(provider, bus()).await).await;
    let pending = parked_schedule(ActionId::new(), &runtime.scheduled_runs).await;
    tokio::task::yield_now().await;

    drop(runtime.scheduled_runs_worker);

    assert_eq!(
        timeout(HANG_GUARD, pending).await.unwrap().unwrap(),
        Err(ScheduleError::SchedulerStopped.to_string()),
    );
}

#[tokio::test]
async fn stopping_scheduled_runs_whose_worker_never_started_returns() {
    let provider: Arc<dyn DataProvider> = Arc::new(StubProvider::idle());
    let runtime = common::frozen_runtime(common::wiring_parts(provider, bus()).await).await;

    drop(runtime.scheduled_runs_worker);

    timeout(HANG_GUARD, runtime.scheduled_runs.stop())
        .await
        .expect("a stop waited for a worker that can never mark itself stopped");
}

#[derive(Default)]
struct RecordingVoice(Mutex<Vec<String>>);

#[async_trait]
impl SpeakRequester for RecordingVoice {
    async fn speak(&self, text: String, _voice: Option<String>) -> Result<(), ScriptSpeakError> {
        self.0.lock().unwrap().push(text);
        Ok(())
    }

    async fn skip(&self) {}

    async fn clear(&self) {}
}

struct NoStoredValues;

impl LatestValueReader for NoStoredValues {
    fn latest(&self, _slot: &str, _scope: LatestScope<'_>) -> Option<LatestValue> {
        None
    }
}

fn bare_parts() -> ScriptRegistryParts {
    ScriptRegistryParts {
        speak_requester: None,
        latest_values: None,
        scheduling: None,
    }
}

async fn script_outcome(registry: ScriptRegistry, body: &str) -> SubActionOutcome {
    use forge_registry::SubActionRunner;
    let (backend, _media) = sqlite().await;
    let bus = bus();
    let runner = ScriptRunInlineRunner::new(
        Arc::new(registry),
        Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
        Arc::clone(&bus) as Arc<dyn EventPublisher>,
        Arc::clone(&backend) as Arc<dyn SettingsRepo>,
    );
    let mut config = runner.default_config();
    config.insert(INLINE_BODY_KEY.to_owned(), Variant::String(body.to_owned()));
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), bus.as_ref());
    runner.execute(&config, &ctx).await.0.outcome
}

#[tokio::test]
async fn a_script_speaks_through_the_voice_the_registry_was_built_with() {
    let voice = Arc::new(RecordingVoice::default());
    let registry = ScriptRegistry::with_parts(ScriptRegistryParts {
        speak_requester: Some(Arc::clone(&voice) as Arc<dyn SpeakRequester>),
        ..bare_parts()
    });

    let outcome = script_outcome(registry, r#"forge::tts::speak("hello");"#).await;

    assert_eq!(
        (outcome, voice.0.lock().unwrap().clone()),
        (SubActionOutcome::Success, vec!["hello".to_owned()]),
    );
}

#[tokio::test]
async fn a_script_reads_latest_values_only_from_a_registry_built_with_a_reader() {
    let reader: Arc<dyn LatestValueReader> = Arc::new(NoStoredValues);
    for (latest_values, succeeds) in [(Some(reader), true), (None, false)] {
        let registry = ScriptRegistry::with_parts(ScriptRegistryParts {
            latest_values,
            ..bare_parts()
        });

        let outcome = script_outcome(registry, r#"forge::latest::get("follower");"#).await;

        assert_eq!(
            matches!(outcome, SubActionOutcome::Success),
            succeeds,
            "{outcome:?}"
        );
    }
}

#[test]
fn a_registry_built_without_scheduling_offers_no_scheduler() {
    let registry = ScriptRegistry::with_parts(bare_parts());

    let scheduler = registry.action_scheduler(SchedulingContext::outside_any_run(ArgStack::new()));

    assert!(scheduler.is_none());
}
