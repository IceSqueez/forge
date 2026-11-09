#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test fixtures"
)]
mod common;
use common::FixedClock;
use common::chain::{chain_step, if_cfg, inline, step};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventPublisher, EventSource, EventSubscription};
use forge_overlay::config::HEADLINE;
use forge_registry::SubActionRunner;
use forge_runtime::{
    ActionEngineHandle, CatchUpSettle, EventBus, NullEventLogRepo, QueueAdmission,
    QueueSchedulerHandle, RuntimeWiringParts, ScheduledRunsHandle, SchedulerRequest, ScriptCatalog,
    ScriptRunInlineRunner, TriggerEvaluatorHandle,
};
use forge_storage::{DataProvider, OverlayId};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    ActionId, ArgStack, EventId, ExecutionContext, ExecutionMetadata, Queue, QueueId,
    RehearsalKind, SubActionConfig, SubActionStep, Variant,
};
use tempfile::TempDir;
use tokio::time::timeout;

const DEADLINE: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(5);
const ACTION_DONE: &str = "action.done";
const IF_KIND: &str = "core.logic.if_then_else";
const OVERLAY_SEND_KIND: &str = "overlay.send";
const CARD_KIND: &str = forge_overlay::kinds::card::KIND_ID;
const CHAT_AND_GLOBAL_SCRIPT: &str =
    r#"forge::chat::send("thanks"); forge::globals::set("shoutouts", 1, false);"#;
const FAILING_SCRIPT: &str = r#"throw "boom";"#;
const HISTORY_LIMIT: u32 = 16;

#[derive(Clone, Copy, Debug)]
enum Root {
    Live,
    TestMarked,
    Replayed,
    ManualTestOfLive,
}

struct Harness {
    backend: Arc<SqliteBackend>,
    _media: TempDir,
    bus: Arc<EventBus>,
    queues: QueueSchedulerHandle,
    queue_id: QueueId,
    _scheduled_runs: ScheduledRunsHandle,
    _step_engine: ActionEngineHandle,
    _evaluator: TriggerEvaluatorHandle,
}

impl Harness {
    async fn new() -> Self {
        let (backend, media) = common::sqlite_backend().await;
        let backend = Arc::new(backend);
        forge_storage::write_setting(
            backend.settings_repo().as_ref(),
            &forge_script::settings::TIMEOUT_MS,
            &10_000,
        )
        .await
        .unwrap();
        let dp: Arc<dyn DataProvider> = Arc::clone(&backend) as Arc<dyn DataProvider>;
        let queue = Queue {
            id: QueueId::new(),
            name: "rehearsal".to_owned(),
            description: String::new(),
            concurrency: std::num::NonZeroU32::new(1).unwrap(),
        };
        dp.queue_repo().save(&queue).await.unwrap();
        let bus = EventBus::new(Arc::new(NullEventLogRepo));
        let runtime = common::frozen_runtime(RuntimeWiringParts {
            paths: forge_paths::AppPaths::at(std::env::temp_dir().join("forge-tests")),
            config: forge_runtime::Config::default(),
            date_names: forge_runtime::DateNamesHandle::default(),
            clock: Arc::new(FixedClock),
            ..common::wiring_parts(dp, Arc::clone(&bus)).await
        })
        .await;
        let queues = runtime.scheduler.clone();
        let scheduled_runs = runtime.scheduled_runs.clone();
        runtime
            .scheduled_runs_worker
            .start(CatchUpSettle::immediately());
        Self {
            backend,
            _media: media,
            bus,
            queues,
            queue_id: queue.id,
            _scheduled_runs: scheduled_runs,
            _step_engine: runtime.action_engine,
            _evaluator: runtime.trigger_evaluator,
        }
    }

    async fn action(&self, sub_actions: Vec<SubActionStep>) -> ActionId {
        let action =
            common::sequential_action(ActionId::new(), "Shoutout", self.queue_id, sub_actions);
        self.backend.action_repo().save(&action).await.unwrap();
        action.id
    }

    fn script_step(&self, body: &str) -> SubActionStep {
        let runner = ScriptRunInlineRunner::new(
            Arc::new(ScriptCatalog::new()),
            self.backend.globals_repo(),
            Arc::clone(&self.bus) as Arc<dyn EventPublisher>,
            self.backend.settings_repo(),
        );
        let mut config = runner.default_config();
        config.insert("body".to_owned(), Variant::String(body.to_owned()));
        step(runner.id(), config)
    }

    async fn root(&self, root: Root, sub: &mut EventSubscription) -> (EventId, bool) {
        let payload = match root {
            Root::TestMarked => serde_json::json!({ "message": "!so", "test": true }),
            _ => serde_json::json!({ "message": "!so" }),
        };
        let event = Event::new(EventSource::Twitch, "twitch.channel.chat.message", payload);
        let id = event.id;
        self.bus.publish(event);
        let manual_test = matches!(root, Root::ManualTestOfLive);
        if !matches!(root, Root::Replayed) {
            return (id, manual_test);
        }
        self.bus.replay_and_publish(id).await.unwrap();
        loop {
            let event = next(sub).await;
            if event.replay {
                return (event.id, manual_test);
            }
        }
    }

    async fn run(&self, action_id: ActionId, root: Root) -> EventId {
        let mut sub = self.bus.subscribe();
        let (trigger, manual_test) = self.root(root, &mut sub).await;
        self.queues
            .dispatch(SchedulerRequest {
                queue_id: self.queue_id,
                action_id,
                trigger_event_id: trigger,
                trigger_kind: None,
                manual_test,
                initial_args: ArgStack::new(),
                admission: QueueAdmission::default(),
            })
            .unwrap();
        until_runs_finish(&mut sub, 1).await;
        trigger
    }

    async fn history_once(&self, action_id: ActionId, runs: usize) -> Vec<ExecutionContext> {
        let repo = self.backend.history_repo();
        timeout(DEADLINE, async {
            loop {
                let stored = repo
                    .recent_for_action(action_id, HISTORY_LIMIT)
                    .await
                    .unwrap();
                if stored.len() >= runs {
                    return stored;
                }
                tokio::time::sleep(POLL).await;
            }
        })
        .await
        .expect("the run history never landed")
    }

    async fn errors_once_one_landed(&self, action_id: ActionId) -> u64 {
        let repo = self.backend.action_repo();
        timeout(DEADLINE, async {
            loop {
                let errors = repo.telemetry(action_id).await.unwrap().errors_7d;
                if errors >= 1 {
                    return errors;
                }
                tokio::time::sleep(POLL).await;
            }
        })
        .await
        .expect("the live run's stats row never landed")
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

fn rehearsal_of(metadata: &ExecutionMetadata) -> (EventId, Option<RehearsalKind>) {
    match metadata {
        ExecutionMetadata::Trigger { event_id, .. } => (*event_id, None),
        ExecutionMetadata::Rehearsal {
            event_id,
            rehearsal,
            ..
        } => (*event_id, Some(*rehearsal)),
        other => panic!("unexpected run metadata {other:?}"),
    }
}

#[tokio::test]
async fn each_run_is_recorded_with_the_rehearsal_kind_of_what_started_it() {
    let harness = Harness::new().await;
    let action = harness.action(Vec::new()).await;
    let mut expected = BTreeMap::new();
    for (root, kind) in [
        (Root::Live, None),
        (Root::TestMarked, Some(RehearsalKind::TestRun)),
        (Root::Replayed, Some(RehearsalKind::Replay)),
        (Root::ManualTestOfLive, Some(RehearsalKind::TestRun)),
    ] {
        let trigger = harness.run(action, root).await;
        expected.insert(trigger.to_string(), kind);
    }

    let recorded: BTreeMap<String, Option<RehearsalKind>> = harness
        .history_once(action, expected.len())
        .await
        .iter()
        .map(|context| {
            let (event_id, kind) = rehearsal_of(&context.metadata);
            (event_id.to_string(), kind)
        })
        .collect();

    assert_eq!(recorded, expected);
}

#[tokio::test]
async fn a_rehearsal_run_leaves_no_row_in_the_actions_run_stats() {
    let harness = Harness::new().await;
    let failing = harness.script_step(FAILING_SCRIPT);
    let action = harness.action(vec![failing]).await;
    harness.run(action, Root::TestMarked).await;
    harness.run(action, Root::Replayed).await;
    harness.run(action, Root::Live).await;
    harness.history_once(action, 3).await;

    let errors = harness.errors_once_one_landed(action).await;

    assert_eq!(errors, 1);
}

fn marks_under(root: EventId, events: &[Event]) -> BTreeMap<bool, BTreeSet<String>> {
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
            .entry(event.is_rehearsal())
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

async fn marks_of_one_run(root: Root) -> BTreeMap<bool, BTreeSet<String>> {
    let harness = Harness::new().await;
    let step = harness.script_step(CHAT_AND_GLOBAL_SCRIPT);
    let action = harness.action(vec![step]).await;
    let mut sub = harness.bus.subscribe();
    let (trigger, manual_test) = harness.root(root, &mut sub).await;
    harness
        .queues
        .dispatch(SchedulerRequest {
            queue_id: harness.queue_id,
            action_id: action,
            trigger_event_id: trigger,
            trigger_kind: None,
            manual_test,
            initial_args: ArgStack::new(),
            admission: QueueAdmission::default(),
        })
        .unwrap();
    let events = until_runs_finish(&mut sub, 1).await;
    marks_under(trigger, &events)
}

#[tokio::test]
async fn every_event_of_a_test_run_carries_the_test_mark() {
    for root in [Root::TestMarked, Root::ManualTestOfLive] {
        assert_eq!(
            marks_of_one_run(root).await,
            BTreeMap::from([(true, whole_run_kinds())]),
            "{root:?}"
        );
    }
}

#[tokio::test]
async fn no_event_of_a_live_run_carries_the_test_mark() {
    assert_eq!(
        marks_of_one_run(Root::Live).await,
        BTreeMap::from([(false, whole_run_kinds())])
    );
}

fn card_send(overlay: &OverlayId) -> SubActionConfig {
    SubActionConfig::from([
        (
            "overlay_id".to_owned(),
            Variant::String(overlay.as_str().to_owned()),
        ),
        (
            HEADLINE.to_owned(),
            Variant::String("Rehearsed headline".to_owned()),
        ),
    ])
}

async fn retained_after_one_run(root: Root, nested: bool) -> bool {
    let harness = Harness::new().await;
    let schema = common::builtin_overlay_kinds()
        .get(CARD_KIND)
        .expect("the card kind ships")
        .config_schema_version();
    let card = harness
        .backend
        .overlay_repo()
        .create("Panel", CARD_KIND, schema)
        .await
        .unwrap();
    let send = card_send(&card.id);
    let steps = if nested {
        vec![step(
            IF_KIND,
            if_cfg(
                "1 == 1",
                inline(vec![chain_step(OVERLAY_SEND_KIND, send)]),
                inline(vec![]),
            ),
        )]
    } else {
        vec![step(OVERLAY_SEND_KIND, send)]
    };
    let action = harness.action(steps).await;

    harness.run(action, root).await;

    harness
        .backend
        .overlay_repo()
        .get_retained_content(&card.id)
        .await
        .unwrap()
        .is_some()
}

#[tokio::test]
async fn an_overlay_card_shown_by_a_test_or_replay_run_keeps_no_retained_content() {
    for root in [Root::TestMarked, Root::Replayed, Root::ManualTestOfLive] {
        for nested in [false, true] {
            assert!(
                !retained_after_one_run(root, nested).await,
                "{root:?}, nested {nested}: the rehearsal became what a reconnecting page opens on"
            );
        }
    }
}

#[tokio::test]
async fn an_overlay_card_shown_by_a_live_run_keeps_it_as_retained_content() {
    for nested in [false, true] {
        assert!(
            retained_after_one_run(Root::Live, nested).await,
            "nested {nested}"
        );
    }
}

#[tokio::test]
async fn an_action_fired_by_a_test_trigger_step_runs_as_a_test_run() {
    let harness = Harness::new().await;
    let fired = harness.action(Vec::new()).await;
    let instance = forge_types::TriggerInstance {
        id: forge_types::TriggerInstanceId::new(),
        kind_id: "twitch.channel.chat.message".to_owned(),
        name: "chat".to_owned(),
        overrides: forge_types::TriggerConfig::new(),
        enabled: true,
        user_defined: true,
        platform_scope: forge_types::PlatformScope::Any,
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: forge_types::PermissionRung::Everyone,
    };
    let instances = harness.backend.trigger_instance_repo();
    instances.save(&instance).await.unwrap();
    instances.link_action(fired, instance.id).await.unwrap();
    let firing = harness
        .action(vec![step(
            "core.test.fire_trigger",
            SubActionConfig::from([(
                "trigger_instance_id".to_owned(),
                Variant::String(instance.id.to_string()),
            )]),
        )])
        .await;

    harness.run(firing, Root::Live).await;

    let kinds: Vec<Option<RehearsalKind>> = harness
        .history_once(fired, 1)
        .await
        .iter()
        .map(|context| rehearsal_of(&context.metadata).1)
        .collect();
    assert_eq!(kinds, [Some(RehearsalKind::TestRun)]);
}
