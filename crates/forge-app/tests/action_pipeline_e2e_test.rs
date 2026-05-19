#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource, EventsError};
use forge_runtime::{
    CommandParser, EventBus, NullEventLogRepo, QueueScheduler, ScriptRegistry, spawn_action_engine,
};
use forge_storage::DataProvider;
use forge_storage_sqlite::SqliteBackend;
use forge_types::{Action, ActionId, Command, CommandId, CommandPermission, SubActionSpec};

const TEST_KEY: [u8; 32] = [0xab; 32];

async fn make_dp() -> Arc<dyn DataProvider> {
    Arc::new(
        SqliteBackend::open_with_key(":memory:", TEST_KEY)
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn full_action_pipeline_emits_causation_chain() {
    let dp = make_dp().await;
    let bus = EventBus::new(Arc::new(NullEventLogRepo));

    let queue = dp
        .queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("Default queue must be seeded by migration 0002_action_engine.sql");

    let action_id = ActionId::new();
    let action = Action {
        id: action_id,
        name: "!quote action".into(),
        group: None,
        queue_id: queue.id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        description: None,
        sub_actions: vec![SubActionSpec::SendChat {
            message: "Hello %user%, you said %args%".into(),
            target: "twitch".into(),
        }],
    };
    dp.action_repo().save(&action).await.unwrap();

    let command = Command {
        id: CommandId::new(),
        action_id,
        name: "!quote".into(),
        cooldown_secs: 0,
        permission: CommandPermission::Everyone,
    };
    dp.command_repo().save(&command).await.unwrap();

    // Subscribe before spawning so that no events published by the runtime are missed.
    let mut sub = bus.subscribe();

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::new(ScriptRegistry::new()),
        None,
    );
    let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
    let _parser = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), scheduler);

    // Yield so that spawned tokio tasks can start processing their subscriptions.
    tokio::time::sleep(Duration::from_millis(10)).await;

    let chat_event = Event::new(
        EventSource::Twitch,
        "chat.message",
        serde_json::json!({
            "message": "!quote some args here",
            "user_login": "alice",
        }),
    );
    let chat_event_id = chat_event.id;
    bus.publish(chat_event);

    let mut received: Vec<Event> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut saw_action_done = false;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, sub.recv()).await {
            Ok(Ok(ev)) => {
                let is_done = ev.kind == "action.done";
                received.push(ev);
                if is_done {
                    saw_action_done = true;
                    break;
                }
            }
            Ok(Err(EventsError::LaggingReceiver)) => {
                // broadcast channel lag: continue draining remaining events
            }
            Ok(Err(_)) | Err(_) => break,
        }
    }

    let kinds: Vec<&str> = received.iter().map(|e| e.kind.as_str()).collect();

    assert!(
        saw_action_done,
        "action.done must arrive within 2s; got: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"command.matched"),
        "missing command.matched; got: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"action.start"),
        "missing action.start; got: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"subaction.run"),
        "missing subaction.run; got: {:?}",
        kinds
    );
    assert!(
        kinds.contains(&"chat.send.request"),
        "missing chat.send.request; got: {:?}",
        kinds
    );

    let cmd_matched = received
        .iter()
        .find(|e| e.kind == "command.matched")
        .unwrap();
    assert_eq!(
        cmd_matched.caused_by,
        Some(chat_event_id),
        "command.matched.caused_by must reference the triggering chat.message event"
    );

    let send_req = received
        .iter()
        .find(|e| e.kind == "chat.send.request")
        .unwrap();
    let msg = send_req.payload["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("Hello alice"),
        "%%user%% interpolation must produce 'Hello alice'; got: {msg}"
    );
    assert!(
        msg.contains("some args here"),
        "%%args%% interpolation must produce 'some args here'; got: {msg}"
    );
}

#[tokio::test]
async fn unknown_command_does_not_dispatch_action() {
    let dp = make_dp().await;
    let bus = EventBus::new(Arc::new(NullEventLogRepo));

    let queue = dp
        .queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("Default queue must be seeded by migration");

    let mut sub = bus.subscribe();

    let engine = spawn_action_engine(
        Arc::clone(&bus),
        Arc::clone(&dp),
        Arc::new(ScriptRegistry::new()),
        None,
    );
    let scheduler = QueueScheduler::spawn(engine, Arc::clone(&bus), vec![queue]);
    let _parser = CommandParser::spawn(Arc::clone(&bus), Arc::clone(&dp), scheduler);

    tokio::time::sleep(Duration::from_millis(10)).await;

    bus.publish(Event::new(
        EventSource::Twitch,
        "chat.message",
        serde_json::json!({
            "message": "!unknown",
            "user_login": "alice",
        }),
    ));

    let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, sub.recv()).await {
            Ok(Ok(ev)) if ev.kind == "command.matched" => {
                panic!("command.matched must not fire for an unregistered command");
            }
            Ok(Ok(_)) | Ok(Err(EventsError::LaggingReceiver)) => {}
            Ok(Err(_)) | Err(_) => break,
        }
    }
}
