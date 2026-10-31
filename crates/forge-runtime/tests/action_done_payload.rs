#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::runners::FixedOutcomeRunner;

use std::sync::Arc;
use std::time::Duration;

use forge_events::Event;
use forge_events::EventSubscription;
use forge_registry::SubActionRegistry;
use forge_runtime::sub_action_runners::{CoreLogicWaitRunner, WAIT_KIND_ID, WAIT_MS_KEY};
use forge_runtime::{
    ActionCancelRegistry, Catalog, EventBus, NullEventLogRepo, QueueAdmission, QueueScheduler,
    QueueSchedulerHandle, SchedulerRequest, spawn_action_engine,
};
use forge_storage::DataProvider;
use forge_types::{
    Action, ActionId, ArgStack, DEFAULT_QUEUE_ID, EventId, Queue, SubActionConfig,
    SubActionOutcome, SubActionStep, Variant,
};
use serde_json::{Value, json};
use tempfile::TempDir;

const SUCCESS_KIND_ID: &str = "test.succeed";
const SKIPPED_KIND_ID: &str = "test.skip";
const FAILING_KIND_ID: &str = "test.fail";
const FAILURE_REASON: &str = "scene not found";
const LONG_WAIT_MS: i64 = 60_000;
const HANG_GUARD: Duration = Duration::from_secs(30);

struct Rig {
    dp: Arc<dyn DataProvider>,
    bus: Arc<EventBus>,
    sched: QueueSchedulerHandle,
    cancel: Arc<ActionCancelRegistry>,
    _media: TempDir,
}

async fn rig() -> Rig {
    let (backend, media) = common::sqlite_backend().await;
    let dp: Arc<dyn DataProvider> = Arc::new(backend);

    let mut registry = SubActionRegistry::new();
    for (id, outcome) in [
        (SUCCESS_KIND_ID, SubActionOutcome::Success),
        (
            SKIPPED_KIND_ID,
            SubActionOutcome::Skipped(forge_types::VerbatimText::new("condition false").into()),
        ),
        (
            FAILING_KIND_ID,
            SubActionOutcome::Failed(forge_types::VerbatimText::new(FAILURE_REASON).into()),
        ),
    ] {
        registry
            .register(Box::new(FixedOutcomeRunner { id, outcome }))
            .unwrap();
    }
    registry.register(Box::new(CoreLogicWaitRunner)).unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let cancel = Arc::new(ActionCancelRegistry::new());
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        dp.action_repo(),
        dp.history_repo(),
        Arc::new(registry),
        Arc::clone(&cancel),
    );
    let sched = QueueScheduler::spawn(
        engine,
        Arc::clone(&bus),
        vec![Queue {
            id: DEFAULT_QUEUE_ID,
            name: "default".to_owned(),
            description: String::new(),
            concurrency: std::num::NonZeroU32::MIN,
        }],
    );
    Rig {
        dp,
        bus,
        sched,
        cancel,
        _media: media,
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

async fn saved_action(rig: &Rig, name: &str, steps: Vec<SubActionStep>) -> Action {
    let action = common::sequential_action(ActionId::new(), name, DEFAULT_QUEUE_ID, steps);
    rig.dp.action_repo().save(&action).await.unwrap();
    action
}

async fn dispatch(rig: &Rig, action: &Action, manual_test: bool) {
    rig.sched
        .dispatch(SchedulerRequest {
            queue_id: action.queue_id,
            action_id: action.id,
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test,
            initial_args: ArgStack::new(),
            admission: QueueAdmission::default(),
        })
        .await
        .unwrap();
}

fn is_kind_for(ev: &Event, kind: &str, action: &Action) -> bool {
    ev.kind == kind && ev.payload.get("action_id") == Some(&json!(action.id.to_string()))
}

async fn events_until(sub: &mut EventSubscription, kind: &str, action: &Action) -> Vec<Event> {
    tokio::time::timeout(HANG_GUARD, async {
        let mut seen = Vec::new();
        loop {
            let ev = sub.recv().await.expect("the bus stays open");
            let matched = is_kind_for(&ev, kind, action);
            seen.push(ev);
            if matched {
                return seen;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {kind} for the action arrived"))
}

async fn done_payload(rig: &Rig, action: &Action, manual_test: bool) -> Value {
    let mut sub = rig.bus.subscribe();
    dispatch(rig, action, manual_test).await;
    events_until(&mut sub, "action.done", action)
        .await
        .pop()
        .expect("events_until returns the matched event last")
        .payload
}

#[tokio::test]
async fn action_done_reports_the_manual_test_flag_the_request_carried() {
    let rig = rig().await;
    let action = saved_action(&rig, "flagged", vec![]).await;

    for manual_test in [true, false] {
        let payload = done_payload(&rig, &action, manual_test).await;
        assert_eq!(
            payload["manual_test"],
            json!(manual_test),
            "dispatched with manual_test={manual_test}"
        );
    }
}

#[tokio::test]
async fn action_done_carries_the_action_display_name() {
    let rig = rig().await;
    let action = saved_action(&rig, "Привіт - shout out ✨", vec![]).await;

    let payload = done_payload(&rig, &action, false).await;

    assert_eq!(payload["action_name"], json!("Привіт - shout out ✨"));
}

#[tokio::test]
async fn action_done_carries_a_reason_only_when_the_run_failed() {
    let rig = rig().await;
    let cases = [
        (SUCCESS_KIND_ID, "success", None),
        (SKIPPED_KIND_ID, "success", None),
        (FAILING_KIND_ID, "failed", Some(FAILURE_REASON)),
    ];

    for (kind_id, outcome, reason) in cases {
        let action = saved_action(&rig, kind_id, vec![step(kind_id, SubActionConfig::new())]).await;
        let payload = done_payload(&rig, &action, false).await;
        assert_eq!(payload["outcome"], json!(outcome), "step {kind_id}");
        assert_eq!(
            payload.get("reason"),
            reason.map(|r| json!(r)).as_ref(),
            "step {kind_id}"
        );
    }
}

#[tokio::test]
async fn cancelled_run_publishes_no_action_done() {
    let rig = rig().await;
    let mut wait = SubActionConfig::new();
    wait.insert(WAIT_MS_KEY.to_owned(), Variant::Int(LONG_WAIT_MS));
    let cancelled = saved_action(&rig, "cancelled", vec![step(WAIT_KIND_ID, wait)]).await;
    let next = saved_action(&rig, "next", vec![]).await;
    let mut sub = rig.bus.subscribe();

    dispatch(&rig, &cancelled, true).await;
    events_until(&mut sub, "action.start", &cancelled).await;
    assert_eq!(rig.cancel.cancel(cancelled.id), 1);
    dispatch(&rig, &next, false).await;
    let seen = events_until(&mut sub, "action.done", &next).await;

    assert!(
        !seen
            .iter()
            .any(|ev| is_kind_for(ev, "action.done", &cancelled)),
        "a cancelled run must not publish action.done before the next run on its queue finishes"
    );
}
