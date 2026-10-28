#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use forge_events::EventSubscription;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{SubActionRegistry, SubActionRunner};
use forge_runtime::sub_action_runners::ScriptRunInlineRunner;
use forge_runtime::{
    ActionCancelRegistry, Catalog, CatchUpSettle, EventBus, NullEventLogRepo, QueueAdmission,
    QueueScheduler, QueueSchedulerHandle, ScheduledRunsCell, ScheduledRunsParts, SchedulerRequest,
    ScriptRegistry, ScriptScheduling, WallClock, spawn_action_engine, spawn_scheduled_runs,
};
use forge_storage::history::MockHistoryRepo;
use forge_storage::{DataProvider, GlobalsRepo, SettingsRepo};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, ArgStack, EventId, ExecutionMode, Queue, QueueId, Variant};
use tempfile::TempDir;
use time::OffsetDateTime;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(30);
const BASE_UNIX_SECS: i64 = 1_791_115_200;
const ACTION_DONE: &str = "action.done";
const SCHEDULE_TARGET: &str = "Greet";
const CHAT_AND_GLOBAL_SCRIPT: &str =
    r#"forge::chat::send("thanks"); forge::globals::set("shoutouts", 1, false);"#;
const SCHEDULE_SCRIPT: &str = r#"forge::schedule::after("Greet", 3600);"#;

struct FixedClock;

impl WallClock for FixedClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(BASE_UNIX_SECS).unwrap()
    }
}

struct Harness {
    backend: Arc<SqliteBackend>,
    _media: TempDir,
    bus: Arc<EventBus>,
    queues: QueueSchedulerHandle,
    queue_id: QueueId,
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
            name: "replay".to_owned(),
            description: String::new(),
            concurrency: 8,
        };
        dp.queue_repo().save(&queue).await.unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let catalog = Catalog::from_provider(dp.as_ref());
        let cell = ScheduledRunsCell::new();
        let mut scripts = ScriptRegistry::new();
        scripts.set_scheduling(ScriptScheduling::new(cell.clone(), dp.action_repo()));
        let mut steps = SubActionRegistry::new();
        steps
            .register(Box::new(ScriptRunInlineRunner::new(
                Arc::new(scripts),
                Arc::clone(&backend) as Arc<dyn GlobalsRepo>,
                Arc::clone(&bus) as Arc<dyn EventPublisher>,
                Arc::clone(&backend) as Arc<dyn SettingsRepo>,
            )))
            .unwrap();
        let mut history = MockHistoryRepo::new();
        history.expect_save_batch().returning(|_| Ok(()));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            Arc::clone(&catalog),
            dp.action_repo(),
            Arc::new(history),
            Arc::new(steps),
            Arc::new(ActionCancelRegistry::new()),
        );
        let queues = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue.clone()]);
        cell.set(spawn_scheduled_runs(ScheduledRunsParts {
            repo: dp.scheduled_run_repo(),
            revision: dp.scheduled_run_revision(),
            catalog,
            actions: dp.action_repo(),
            queues: queues.clone(),
            bus: Arc::clone(&bus),
            clock: Arc::new(FixedClock),
            catch_up: CatchUpSettle::immediately(),
        }));
        Self {
            backend,
            _media: media,
            bus,
            queues,
            queue_id: queue.id,
        }
    }

    async fn action(&self, name: &str, script: Option<&str>) -> ActionId {
        let runner = ScriptRunInlineRunner::new(
            Arc::new(ScriptRegistry::new()),
            Arc::clone(&self.backend) as Arc<dyn GlobalsRepo>,
            Arc::clone(&self.bus) as Arc<dyn EventPublisher>,
            Arc::clone(&self.backend) as Arc<dyn SettingsRepo>,
        );
        let sub_actions = script
            .map(|body| {
                let mut config = runner.default_config();
                config.insert("body".to_owned(), Variant::String(body.to_owned()));
                forge_types::SubActionStep {
                    kind_id: runner.id().to_owned(),
                    config,
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                }
            })
            .into_iter()
            .collect();
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
            sub_actions,
        };
        self.backend.action_repo().save(&action).await.unwrap();
        action.id
    }

    async fn live_and_replayed_roots(&self, sub: &mut EventSubscription) -> (EventId, EventId) {
        let live = Event::new(
            EventSource::Twitch,
            "twitch.channel.chat.message",
            serde_json::json!({ "message": "!so" }),
        );
        let live_id = live.id;
        self.bus.publish(live);
        self.bus.replay_and_publish(live_id).await.unwrap();
        let mut replayed = None;
        while replayed.is_none() {
            let event = next(sub).await;
            replayed = event.replay.then_some(event.id);
        }
        (live_id, replayed.unwrap())
    }

    async fn dispatch(&self, action_id: ActionId, trigger: EventId) {
        self.queues
            .dispatch(SchedulerRequest {
                queue_id: self.queue_id,
                action_id,
                trigger_event_id: trigger,
                trigger_kind: None,
                manual_test: false,
                initial_args: ArgStack::new(),
                admission: QueueAdmission::default(),
            })
            .await
            .unwrap();
    }
}

async fn next(sub: &mut EventSubscription) -> Event {
    timeout(DEADLINE, sub.recv())
        .await
        .expect("the bus went quiet")
        .expect("the bus closed")
}

async fn until_runs_finish(sub: &mut EventSubscription, runs: usize) -> Vec<Event> {
    let mut seen = Vec::new();
    let mut done = 0;
    while done < runs {
        let event = next(sub).await;
        if event.kind == ACTION_DONE {
            done += 1;
        }
        seen.push(event);
    }
    seen
}

fn kinds_by_mark_under(root: EventId, events: &[Event]) -> BTreeMap<bool, BTreeSet<String>> {
    let parents: HashMap<EventId, Option<EventId>> = events
        .iter()
        .map(|event| (event.id, event.caused_by))
        .collect();
    let descends_from_root = |event: &Event| {
        let mut cursor = event.caused_by;
        while let Some(id) = cursor {
            if id == root {
                return true;
            }
            cursor = parents.get(&id).copied().flatten();
        }
        false
    };
    let mut marks: BTreeMap<bool, BTreeSet<String>> = BTreeMap::new();
    for event in events.iter().filter(|event| descends_from_root(event)) {
        marks
            .entry(event.replay)
            .or_default()
            .insert(event.kind.clone());
    }
    marks
}

fn whole_run_kinds() -> BTreeSet<String> {
    [
        "action.start",
        "subaction.run",
        "subaction.done",
        "script.exec",
        "chat.send.request",
        "global.set",
        ACTION_DONE,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[tokio::test]
async fn every_event_of_a_replayed_run_carries_the_replay_mark() {
    let harness = Harness::new().await;
    let shoutout = harness
        .action("Shoutout", Some(CHAT_AND_GLOBAL_SCRIPT))
        .await;
    let mut sub = harness.bus.subscribe();
    let (_, replayed) = harness.live_and_replayed_roots(&mut sub).await;

    harness.dispatch(shoutout, replayed).await;
    let events = until_runs_finish(&mut sub, 1).await;

    assert_eq!(
        kinds_by_mark_under(replayed, &events),
        BTreeMap::from([(true, whole_run_kinds())])
    );
}

#[tokio::test]
async fn a_live_run_interleaved_with_a_replayed_run_stays_unmarked() {
    let harness = Harness::new().await;
    let shoutout = harness
        .action("Shoutout", Some(CHAT_AND_GLOBAL_SCRIPT))
        .await;
    let mut sub = harness.bus.subscribe();
    let (live, replayed) = harness.live_and_replayed_roots(&mut sub).await;

    harness.dispatch(shoutout, replayed).await;
    harness.dispatch(shoutout, live).await;
    let events = until_runs_finish(&mut sub, 2).await;

    assert_eq!(
        kinds_by_mark_under(live, &events),
        BTreeMap::from([(false, whole_run_kinds())])
    );
}

#[tokio::test]
async fn a_run_scheduled_from_a_replayed_step_is_stored_as_replay_and_a_live_one_is_not() {
    let harness = Harness::new().await;
    harness.action(SCHEDULE_TARGET, None).await;
    let scheduler = harness.action("Raid", Some(SCHEDULE_SCRIPT)).await;
    let mut sub = harness.bus.subscribe();
    let (live, replayed) = harness.live_and_replayed_roots(&mut sub).await;

    harness.dispatch(scheduler, replayed).await;
    harness.dispatch(scheduler, live).await;
    until_runs_finish(&mut sub, 2).await;

    let stored: BTreeMap<String, bool> = harness
        .backend
        .scheduled_run_repo()
        .list_pending()
        .await
        .unwrap()
        .into_iter()
        .map(|run| {
            (
                run.spec.trigger_event_id.unwrap().to_string(),
                run.spec.replay,
            )
        })
        .collect();
    assert_eq!(
        stored,
        BTreeMap::from([(replayed.to_string(), true), (live.to_string(), false)])
    );
}
