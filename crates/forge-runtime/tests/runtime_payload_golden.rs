#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSubscription, EventsError};
use forge_runtime::{
    EventBus, FrozenRuntime, NullEventLogRepo, QueueAdmission, QueueMode, SchedulerRequest,
    ScriptRegistry,
};
use forge_storage::{DataProvider, ScriptRecord};
use forge_storage_sqlite::SqliteBackend;
use forge_types::{
    Action, ArgStack, EventId, ExecutionMode, Queue, ScriptContract, SubActionStep, Variant,
};
use serde_json::{Value, json};
use tempfile::TempDir;
use time::OffsetDateTime;

const TEST_KEY: [u8; 32] = [0xab; 32];
const HANG_GUARD: Duration = Duration::from_secs(30);
const ACTION_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0AA";
const QUEUE_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0QQ";
const SCRIPT_ID: &str = "01JBZ8Q4T5N7W3K2M9X6V1R0SS";

struct Rig {
    bus: Arc<EventBus>,
    queue: Queue,
    runtime: FrozenRuntime,
    _media: TempDir,
}

async fn rig() -> Rig {
    let media = tempfile::tempdir().unwrap();
    let backend =
        SqliteBackend::open_for_test(":memory:", TEST_KEY, media.path().join("media"), None)
            .await
            .unwrap();
    let dp: Arc<dyn DataProvider> = Arc::new(backend);
    let queue = Queue {
        id: QUEUE_ID.parse().unwrap(),
        name: "golden".to_owned(),
        description: String::new(),
        concurrency: std::num::NonZeroU32::MIN,
    };
    dp.queue_repo().save(&queue).await.unwrap();
    dp.action_repo()
        .save(&Action {
            id: ACTION_ID.parse().unwrap(),
            name: "Golden run".to_owned(),
            group: None,
            queue_id: queue.id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: Vec::new(),
        })
        .await
        .unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let runtime = common::frozen_runtime(common::wiring_parts(dp, Arc::clone(&bus)).await).await;
    Rig {
        bus,
        queue,
        runtime,
        _media: media,
    }
}

async fn collect(sub: &mut EventSubscription, kind: &str, count: usize) -> Vec<Event> {
    let mut seen = Vec::new();
    tokio::time::timeout(HANG_GUARD, async {
        while seen.len() < count {
            match sub.recv().await {
                Ok(event) if event.kind == kind => seen.push(event),
                Ok(_) | Err(EventsError::LaggingReceiver) => {}
                Err(e) => panic!("bus failed: {e:?}"),
            }
        }
    })
    .await
    .expect("the producer never emitted the pinned kind");
    seen
}

fn with_keys_sorted(value: Value) -> Value {
    match value {
        Value::Object(fields) => {
            let sorted: BTreeMap<String, Value> = fields
                .into_iter()
                .map(|(key, value)| (key, with_keys_sorted(value)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(with_keys_sorted).collect()),
        other => other,
    }
}

fn assert_matches_golden(events: &[Event], fixture: &str) {
    let mut emitted: Vec<Value> = events
        .iter()
        .map(|event| json!({ "source": event.source, "payload": event.payload }))
        .collect();
    emitted.sort_by_key(Value::to_string);
    let actual = format!(
        "{}\n",
        serde_json::to_string_pretty(&with_keys_sorted(Value::Array(emitted))).unwrap()
    );
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/observability")
        .join(fixture);
    let golden = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        golden == actual,
        "{fixture} drifted:\n--- golden\n{golden}--- emitted\n{actual}"
    );
}

fn chat_step(entries: &[(&str, &str)]) -> SubActionStep {
    SubActionStep {
        kind_id: "twitch.chat.send_message".to_owned(),
        config: entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), Variant::String((*value).to_owned())))
            .collect(),
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

async fn run_quick_chat_sends(rig: &Rig) -> (Vec<Event>, Vec<Event>) {
    let mut requests = rig.bus.subscribe();
    let mut done = rig.bus.subscribe();
    for (step, label) in [
        (
            chat_step(&[
                ("message", "golden hello"),
                ("target", "twitch"),
                ("sender", "bot"),
            ]),
            "Say hello as bot",
        ),
        (
            chat_step(&[
                ("message", "golden kick"),
                ("target", "kick"),
                ("sender", "default"),
            ]),
            "Say hello on kick",
        ),
    ] {
        rig.runtime
            .action_engine
            .execute_quick_action(step, "twitch".to_owned(), label.to_owned(), None)
            .await
            .unwrap();
    }
    (
        collect(&mut requests, "chat.send.request", 2).await,
        collect(&mut done, "action.quick.done", 2).await,
    )
}

#[tokio::test]
async fn chat_send_step_request_payloads_match_the_golden() {
    let rig = rig().await;

    let (requests, _) = run_quick_chat_sends(&rig).await;

    assert_matches_golden(&requests, "chat.send.request.json");
}

#[tokio::test]
async fn quick_action_done_payloads_match_the_golden() {
    let rig = rig().await;

    let (_, done) = run_quick_chat_sends(&rig).await;

    assert_matches_golden(&done, "action.quick.done.json");
}

#[tokio::test]
async fn paused_queue_skip_payload_matches_the_golden() {
    let rig = rig().await;
    let scheduler = &rig.runtime.scheduler;
    let mut sub = rig.bus.subscribe();
    scheduler
        .set_mode(rig.queue.id, QueueMode::PAUSED)
        .await
        .unwrap();

    scheduler
        .dispatch(SchedulerRequest {
            queue_id: rig.queue.id,
            action_id: ACTION_ID.parse().unwrap(),
            trigger_event_id: EventId::new(),
            trigger_kind: None,
            manual_test: false,
            initial_args: ArgStack::new(),
            admission: QueueAdmission::default(),
        })
        .await
        .unwrap();

    assert_matches_golden(
        &collect(&mut sub, "action.skipped", 1).await,
        "action.skipped.json",
    );
}

#[tokio::test]
async fn script_reload_payload_matches_the_golden() {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let mut sub = bus.subscribe();
    let stamp = OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap();

    ScriptRegistry::new()
        .reload(
            ScriptRecord {
                id: serde_json::from_value(json!(SCRIPT_ID)).unwrap(),
                name: "golden_script".to_owned(),
                body: "let x = 1;".to_owned(),
                contract: ScriptContract::default(),
                enabled: true,
                created_at: stamp,
                last_modified: stamp,
            },
            &bus,
        )
        .await
        .unwrap();

    assert_matches_golden(
        &collect(&mut sub, "script.reloaded", 1).await,
        "script.reloaded.json",
    );
}
