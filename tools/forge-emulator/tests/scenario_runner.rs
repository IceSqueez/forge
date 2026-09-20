//! Step execution against a pretend forge: a scripted control peer plus an EventSub client on the
//! real fake Twitch. Nothing here starts the real forge binary.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use forge_emulator::control::{ClientTimeouts, ControlClient, ControlEndpoint};
use forge_emulator::fixture::{SeedReport, SeededCommand, SeededServer, TwitchAccount};
use forge_emulator::overlay::OverlayPages;
use forge_emulator::run::{
    ActionDetail, ActionIndex, ActionReport, FailureCause, Journal, RunClock, Session, StepOutcome,
    StepStatus, Verdict, execute_steps,
};
use forge_emulator::scenario::{Scenario, parse_scenario};
use forge_emulator::twitch::{FakeTwitch, FakeTwitchConfig};
use forge_events::{Event, EventSource};
use forge_types::{ActionId, EventId, TriggerInstanceId};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use time::format_description::well_known::Rfc3339;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const DEADLINE: Duration = Duration::from_secs(20);
const CHAT: &str = "channel.chat.message";

#[derive(Debug, Clone, Copy)]
struct Behaviour {
    runs_commands: bool,
    refuses_actions: bool,
}

const COOPERATIVE: Behaviour = Behaviour {
    runs_commands: true,
    refuses_actions: false,
};

fn push_frame(event: &Event) -> Value {
    let mut envelope = json!({
        "source": event.source,
        "type": event.kind,
        "id": event.id,
        "replay": event.replay,
    });
    if let Some(cause) = event.caused_by {
        envelope["causedBy"] = json!(cause);
    }
    json!({
        "timeStamp": event.timestamp.format(&Rfc3339).unwrap(),
        "event": envelope,
        "data": event.payload,
    })
}

/// Answers control requests the way forge's server does and publishes the events forge would;
/// pushing `null` closes the connection.
struct PretendForge {
    endpoint: ControlEndpoint,
    requests: mpsc::UnboundedReceiver<Value>,
    pushes: mpsc::UnboundedSender<Value>,
}

impl PretendForge {
    async fn start(behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (requests_tx, requests) = mpsc::unbounded_channel();
        let (pushes, mut pushes_rx) = mpsc::unbounded_channel::<Value>();
        let publisher = pushes.clone();
        tokio::spawn(async move {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            loop {
                tokio::select! {
                    incoming = socket.next() => match incoming {
                        Some(Ok(Message::Text(text))) => {
                            let request: Value = serde_json::from_str(text.as_str()).unwrap();
                            let reply = answer(&request, behaviour, &publisher);
                            let _ = requests_tx.send(request);
                            if socket.send(Message::Text(reply.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(_)) => {}
                        _ => break,
                    },
                    push = pushes_rx.recv() => match push {
                        Some(Value::Null) => {
                            let _ = socket.close(None).await;
                            break;
                        }
                        Some(frame) => {
                            if socket.send(Message::Text(frame.to_string().into())).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    },
                }
            }
        });
        Self {
            endpoint: ControlEndpoint::loopback("127.0.0.1", port).unwrap(),
            requests,
            pushes,
        }
    }

    fn publish(&self, event: &Event) {
        self.pushes.send(push_frame(event)).unwrap();
    }

    fn requested(&mut self) -> Vec<String> {
        let mut methods = Vec::new();
        while let Ok(request) = self.requests.try_recv() {
            methods.push(request["request"].as_str().unwrap_or_default().to_owned());
        }
        methods
    }

    fn follow_twitch(&self, fake: &FakeTwitch, behaviour: Behaviour) {
        let pushes = self.pushes.clone();
        let socket_url = fake.eventsub_ws_url().to_owned();
        let api = fake.api_base_url().to_owned();
        tokio::spawn(async move {
            let (mut socket, session_id) = open_session(&socket_url).await;
            subscribe_to_chat(&api, &session_id).await;
            while let Some(frame) = next_frame(&mut socket).await {
                match frame["metadata"]["message_type"].as_str() {
                    Some("notification") => {
                        for event in chat_effects(&frame["payload"]["event"], behaviour) {
                            let _ = pushes.send(push_frame(&event));
                        }
                    }
                    Some("session_reconnect") => {
                        let url = frame["payload"]["session"]["reconnect_url"]
                            .as_str()
                            .unwrap()
                            .to_owned();
                        socket = open_session(&url).await.0;
                    }
                    _ => {}
                }
            }
        });
    }
}

fn answer(
    request: &Value,
    behaviour: Behaviour,
    publisher: &mpsc::UnboundedSender<Value>,
) -> Value {
    let id = request["id"].clone();
    let ok = |fields: Value| {
        let mut frame = fields;
        frame["id"] = id.clone();
        frame["status"] = json!("ok");
        frame
    };
    match request["request"].as_str() {
        Some("doAction") if behaviour.refuses_actions => json!({
            "id": id,
            "status": "error",
            "error": { "code": "NOT_FOUND", "message": "action not found" },
        }),
        Some("doAction") => {
            let execution = EventId::new();
            let started = Event::caused_by(
                EventSource::Core,
                "action.start",
                json!({ "action_id": request["actionId"], "action_name": "Ping" }),
                execution,
            );
            publisher.send(push_frame(&started)).unwrap();
            ok(json!({ "ok": true, "execution_id": execution.to_string() }))
        }
        Some("setGlobal") => {
            let set = Event::new(
                EventSource::Server,
                "global.set",
                json!({ "key": request["name"], "new_value": request["value"] }),
            );
            publisher.send(push_frame(&set)).unwrap();
            ok(json!({ "ok": true }))
        }
        _ => ok(json!({})),
    }
}

fn chat_effects(notification: &Value, behaviour: Behaviour) -> Vec<Event> {
    let text = notification["message"]["text"].as_str().unwrap_or_default();
    let chat = Event::new(
        EventSource::Twitch,
        "chat.message",
        json!({
            "message": text,
            "user": { "login": notification["chatter_user_login"] },
        }),
    );
    let mut effects = vec![chat.clone()];
    if behaviour.runs_commands && text.starts_with("!ping") {
        effects.push(Event::caused_by(
            EventSource::Core,
            "action.start",
            json!({ "action_name": "Ping" }),
            chat.id,
        ));
    }
    effects
}

async fn next_frame(socket: &mut Socket) -> Option<Value> {
    loop {
        match socket.next().await? {
            Ok(Message::Text(text)) => return serde_json::from_str(text.as_str()).ok(),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

async fn open_session(url: &str) -> (Socket, String) {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let welcome = next_frame(&mut socket).await.unwrap();
    let session_id = welcome["payload"]["session"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (socket, session_id)
}

async fn subscribe_to_chat(api: &str, session_id: &str) {
    let account = TwitchAccount::default();
    let status = reqwest::Client::new()
        .post(format!("{api}/helix/eventsub/subscriptions"))
        .header("Authorization", format!("Bearer {}", account.access_token))
        .header("Client-Id", account.client_id)
        .json(&json!({
            "type": CHAT,
            "version": "1",
            "condition": { "broadcaster_user_id": account.user_id, "user_id": account.user_id },
            "transport": { "method": "websocket", "session_id": session_id },
        }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status.as_u16(), 202);
}

struct Harness {
    forge: PretendForge,
    fake: Option<FakeTwitch>,
    client: ControlClient,
    journal: Journal,
    actions: ActionIndex,
    pages: OverlayPages,
    ping: ActionId,
    logs: TempDir,
}

impl Harness {
    async fn start(behaviour: Behaviour, with_twitch: bool) -> Self {
        let forge = PretendForge::start(behaviour).await;
        let fake = if with_twitch {
            let fake = FakeTwitch::start(FakeTwitchConfig::for_account(&TwitchAccount::default()))
                .await
                .unwrap();
            forge.follow_twitch(&fake, behaviour);
            Some(fake)
        } else {
            None
        };
        let timeouts = ClientTimeouts {
            connect: DEADLINE,
            request: DEADLINE,
        };
        let (client, events) = ControlClient::connect(&forge.endpoint, timeouts)
            .await
            .unwrap();
        let (journal, _feeder) = Journal::follow(events);
        let ping = ActionId::new();
        let seed = SeedReport {
            data_dir: PathBuf::from("/nonexistent-fixture/data"),
            server: SeededServer {
                port: 1,
                bearer_token: "fixture-bearer".to_owned(),
            },
            twitch: None,
            overlays: Vec::new(),
            chat_commands: vec![SeededCommand {
                phrase: "!ping".to_owned(),
                action_name: "Ping".to_owned(),
                action_id: ping,
                trigger_instance_id: TriggerInstanceId::new(),
            }],
        };
        let actions = ActionIndex::from_seed(&seed);
        let pages = OverlayPages::for_seed(&seed).unwrap();
        Self {
            forge,
            fake,
            client,
            journal,
            actions,
            pages,
            ping,
            logs: tempfile::tempdir().unwrap(),
        }
    }

    fn session(&self) -> Session<'_> {
        Session {
            client: &self.client,
            journal: &self.journal,
            twitch: self.fake.as_ref(),
            actions: &self.actions,
            pages: &self.pages,
            log_dir: self.logs.path().to_owned(),
            clock: RunClock::starting_now(),
            ready_at: Instant::now(),
            ready: ActionDetail::ForgeReady {
                attempts: 1,
                server_port: 1,
            },
        }
    }

    async fn run(&self, scenario: &Scenario) -> Vec<StepOutcome> {
        timeout(
            DEADLINE,
            execute_steps(scenario, &self.session(), std::future::pending()),
        )
        .await
        .expect("steps finish in time")
    }
}

fn scenario(fixture: Value, fakes: Value, steps: Value) -> Scenario {
    let text = json!({
        "name": "runner test",
        "purpose": "exercise step execution",
        "fixture": fixture,
        "fakes": fakes,
        "steps": steps,
    })
    .to_string();
    parse_scenario(Path::new("inline.json"), &text).unwrap()
}

fn twitch_scenario(steps: Value) -> Scenario {
    scenario(
        json!({ "twitch": {}, "chat_commands": [{ "phrase": "!ping", "action_name": "Ping" }] }),
        json!({ "twitch": {} }),
        steps,
    )
}

fn statuses(steps: &[StepOutcome]) -> Vec<StepStatus> {
    steps.iter().map(|step| step.status).collect()
}

fn verdicts(step: &StepOutcome) -> Vec<Verdict> {
    step.expectations
        .iter()
        .map(|expectation| expectation.verdict.clone())
        .collect()
}

fn ready() -> Value {
    json!({ "do": { "forge_ready": { "within_ms": 1000 } } })
}

fn subscribed() -> Value {
    json!({
        "do": { "twitch_subscribed": { "types": [CHAT], "within_ms": 5000 } },
        "expect": [
            { "twitch_request_count": { "method": "POST", "path": "/helix/eventsub/subscriptions", "min": 1 } }
        ]
    })
}

fn ping_chat(expect: Value) -> Value {
    json!({
        "do": { "chat": { "viewer": { "user_id": "200000042", "login": "alice" }, "text": "!ping" } },
        "expect": expect
    })
}

#[tokio::test]
async fn scenario_whose_every_expectation_holds_passes_each_step_in_order() {
    let harness = Harness::start(COOPERATIVE, true).await;
    let scenario = twitch_scenario(json!([
        ready(),
        subscribed(),
        ping_chat(json!([
            { "event": { "name": "chat", "source": "twitch", "kind": "chat.message",
                         "payload": { "/message": { "equals": "!ping" }, "/user/login": { "equals": "alice" } },
                         "within_ms": 5000 } },
            { "event": { "name": "started", "source": "core", "kind": "action.start",
                         "payload": { "/action_name": { "equals": "Ping" } }, "within_ms": 5000 } },
            { "caused_by": { "effect": "started", "cause": "chat" } },
            { "event_absent": { "kind": "trigger.blocked", "window_ms": 50 } },
            { "twitch_no_unexpected_requests": {} }
        ])),
        {
            "do": { "run_action": { "action": "Ping", "args": { "x": 1 } } },
            "expect": [ { "event": { "kind": "action.start", "within_ms": 5000 } } ]
        },
        {
            "do": { "set_global": { "name": "mood", "value": "calm" } },
            "expect": [ { "event": { "kind": "global.set", "payload": { "/key": { "equals": "mood" } }, "within_ms": 5000 } } ]
        }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(statuses(&steps), [StepStatus::Passed; 5], "{steps:#?}");
    assert!(
        matches!(&steps[3].action, Some(ActionReport::Done(ActionDetail::ActionRun { action_id, .. }))
            if *action_id == harness.ping),
        "{:?}",
        steps[3].action
    );
}

#[tokio::test]
async fn a_failed_expectation_fails_its_step_and_later_steps_never_run() {
    let mut harness = Harness::start(
        Behaviour {
            runs_commands: false,
            ..COOPERATIVE
        },
        true,
    )
    .await;
    let scenario = twitch_scenario(json!([
        ready(),
        subscribed(),
        ping_chat(json!([
            { "event": { "name": "chat", "kind": "chat.message", "within_ms": 5000 } },
            { "event": { "name": "started", "kind": "action.start", "within_ms": 200 } },
            { "caused_by": { "effect": "started", "cause": "chat" } }
        ])),
        { "do": { "run_action": { "action": "Ping" } } }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        statuses(&steps),
        [
            StepStatus::Passed,
            StepStatus::Passed,
            StepStatus::Failed,
            StepStatus::NotRun
        ]
    );
    assert_eq!(
        verdicts(&steps[2]),
        [
            Verdict::Passed,
            Verdict::Failed(FailureCause::NotObserved {
                needed: 1,
                observed: 0
            }),
            Verdict::Failed(FailureCause::UnresolvedName {
                name: "started".to_owned()
            }),
        ]
    );
    assert!(!harness.forge.requested().iter().any(|m| m == "doAction"));
}

#[tokio::test]
async fn session_reconnect_completes_once_forge_opens_the_successor_session() {
    let harness = Harness::start(COOPERATIVE, true).await;
    let scenario = twitch_scenario(json!([
        ready(),
        subscribed(),
        {
            "do": { "session_reconnect": { "within_ms": 5000 } },
            "expect": [ { "twitch_subscription": { "type": CHAT, "within_ms": 5000 } } ]
        },
        ping_chat(json!([ { "event": { "kind": "chat.message", "within_ms": 5000 } } ]))
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(statuses(&steps), [StepStatus::Passed; 4], "{steps:#?}");
    let Some(ActionReport::Done(ActionDetail::TwitchSubscribed { session_ids })) = &steps[1].action
    else {
        panic!("{:?}", steps[1].action);
    };
    assert!(
        matches!(&steps[2].action, Some(ActionReport::Done(ActionDetail::SessionReconnected { from_session, to_session }))
            if session_ids == std::slice::from_ref(from_session) && to_session != from_session),
        "{:?}",
        steps[2].action
    );
}

#[tokio::test]
async fn crowd_delivers_every_planned_message_to_forge() {
    let harness = Harness::start(COOPERATIVE, true).await;
    let scenario = twitch_scenario(json!([
        ready(),
        subscribed(),
        {
            "do": { "crowd": { "viewers": 5, "chatter": ["hi from {login}"], "chatter_per_viewer": 2, "spacing_ms": 1 } },
            "expect": [ { "event": { "kind": "chat.message", "count": { "exactly": 10 }, "within_ms": 300 } } ]
        }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(verdicts(&steps[2]), [Verdict::Passed], "{:#?}", steps[2]);
    assert!(matches!(
        steps[2].action,
        Some(ActionReport::Done(ActionDetail::CrowdSent { messages: 10 }))
    ));
}

#[tokio::test]
async fn refused_run_action_fails_the_step_without_evaluating_its_expectations() {
    let harness = Harness::start(
        Behaviour {
            refuses_actions: true,
            ..COOPERATIVE
        },
        false,
    )
    .await;
    let scenario = scenario(
        json!({ "chat_commands": [{ "phrase": "!ping", "action_name": "Ping" }] }),
        json!({}),
        json!([
            ready(),
            {
                "do": { "run_action": { "action": "Ping" } },
                "expect": [ { "event": { "kind": "action.start", "within_ms": 5000 } } ]
            }
        ]),
    );

    let steps = harness.run(&scenario).await;

    assert_eq!(statuses(&steps), [StepStatus::Passed, StepStatus::Failed]);
    assert!(
        matches!(&steps[1].action, Some(ActionReport::Failed { reason, .. }) if reason.contains("action not found")),
        "{:?}",
        steps[1].action
    );
    assert_eq!(verdicts(&steps[1]), [Verdict::NotEvaluated]);
}

#[tokio::test]
async fn a_server_drop_notice_inside_the_window_fails_an_absence_check() {
    let harness = Harness::start(COOPERATIVE, false).await;
    let scenario = scenario(
        json!({}),
        json!({}),
        json!([{
            "do": { "forge_ready": { "within_ms": 1000 } },
            "expect": [ { "event_absent": { "kind": "trigger.blocked", "window_ms": 300 } } ]
        }]),
    );
    harness.forge.pushes.send(json!({ "dropped": 2 })).unwrap();

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[0]),
        [Verdict::Failed(FailureCause::StreamGap {
            dropped: 2,
            undecodable: 0
        })]
    );
}

#[tokio::test]
async fn stop_interrupts_the_running_step_and_skips_the_rest() {
    let harness = Harness::start(COOPERATIVE, false).await;
    let scenario = scenario(
        json!({ "chat_commands": [{ "phrase": "!ping", "action_name": "Ping" }] }),
        json!({}),
        json!([
            ready(),
            { "do": { "pause": { "ms": 5000, "reason": "long enough to be interrupted" } } },
            { "do": { "run_action": { "action": "Ping" } } }
        ]),
    );
    let started = Instant::now();

    let steps = timeout(
        DEADLINE,
        execute_steps(
            &scenario,
            &harness.session(),
            tokio::time::sleep(Duration::from_millis(20)),
        ),
    )
    .await
    .unwrap();

    assert_eq!(
        statuses(&steps),
        [
            StepStatus::Passed,
            StepStatus::Interrupted,
            StepStatus::NotRun
        ]
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn events_published_before_a_step_starts_never_satisfy_its_expectations() {
    let harness = Harness::start(COOPERATIVE, false).await;
    let scenario = scenario(
        json!({}),
        json!({}),
        json!([
            {
                "do": { "forge_ready": { "within_ms": 1000 } },
                "expect": [ { "event": { "kind": "custom.early", "within_ms": 5000 } } ]
            },
            {
                "do": { "pause": { "ms": 1, "reason": "separates the steps" } },
                "expect": [ { "event": { "kind": "custom.early", "within_ms": 200 } } ]
            }
        ]),
    );
    harness
        .forge
        .publish(&Event::new(EventSource::Server, "custom.early", json!({})));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[1]),
        [Verdict::Failed(FailureCause::NotObserved {
            needed: 1,
            observed: 0
        })]
    );
}

#[tokio::test]
async fn forge_closing_the_control_connection_ends_waiting_expectations_as_a_closed_stream() {
    let harness = Harness::start(COOPERATIVE, false).await;
    let scenario = scenario(
        json!({}),
        json!({}),
        json!([{
            "do": { "forge_ready": { "within_ms": 1000 } },
            "expect": [ { "event": { "kind": "custom.never", "within_ms": 10000 } } ]
        }]),
    );
    harness.forge.pushes.send(Value::Null).unwrap();
    let started = Instant::now();

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[0]),
        [Verdict::Failed(FailureCause::StreamClosed)]
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}
