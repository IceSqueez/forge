#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use forge_emulator::control::{ClientTimeouts, ControlClient, ControlEndpoint, Observation};
use forge_emulator::discord::FakeDiscord;
use forge_emulator::donatello::{FAKE_DONATELLO_TOKEN, FakeDonatello, FakeDonatelloConfig};
use forge_emulator::fixture::{SeedReport, SeededCommand, SeededServer, TwitchAccount};
use forge_emulator::kick::FakeKick;
use forge_emulator::monobank::{FAKE_MONOBANK_TOKEN, FakeJar, FakeMonobank, FakeMonobankConfig};
use forge_emulator::obs::{FakeObs, FakeObsConfig};
use forge_emulator::overlay::OverlayPages;
use forge_emulator::run::{
    ActionDetail, ActionIndex, ActionReport, DonationFakes, FailureCause, ForgeHost, Journal,
    RunClock, Session, StepOutcome, StepStatus, Verdict, execute_steps,
};
use forge_emulator::scenario::{Scenario, parse_scenario};
use forge_emulator::twitch::{FakeTwitch, FakeTwitchConfig};
use forge_emulator::vtube::{FakeVTube, FakeVTubeConfig};
use forge_emulator::youtube::{FakeYouTube, FakeYouTubeConfig};
use forge_events::{Event, EventSource};
use forge_types::{ActionId, EventId, TriggerInstanceId};
use futures_util::{SinkExt, StreamExt};
use reqwest::Response;
use serde_json::{Value, json};
use tempfile::TempDir;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{Instant, timeout};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const DEADLINE: Duration = Duration::from_secs(20);
const CHAT: &str = "channel.chat.message";
const SUBSCRIBE: &str = "channel.subscribe";

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
            for subscription_type in [CHAT, SUBSCRIBE] {
                subscribe_session(&api, &session_id, subscription_type).await;
            }
            while let Some(frame) = next_frame(&mut socket).await {
                match frame["metadata"]["message_type"].as_str() {
                    Some("notification") => {
                        let event = &frame["payload"]["event"];
                        let effects = match frame["metadata"]["subscription_type"].as_str() {
                            Some(SUBSCRIBE) => vec![subscriber_effect(event)],
                            _ => chat_effects(event, behaviour),
                        };
                        for event in effects {
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

fn subscriber_effect(notification: &Value) -> Event {
    Event::new(
        EventSource::Twitch,
        "twitch.channel.subscribe",
        json!({
            "user": {
                "login": notification["user_login"],
                "display_name": notification["user_name"],
            },
            "tier": notification["tier"],
        }),
    )
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

async fn subscribe_session(api: &str, session_id: &str, subscription_type: &str) {
    let account = TwitchAccount::default();
    let status = reqwest::Client::new()
        .post(format!("{api}/helix/eventsub/subscriptions"))
        .header("Authorization", format!("Bearer {}", account.access_token))
        .header("Client-Id", account.client_id)
        .json(&json!({
            "type": subscription_type,
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
    discord: Option<FakeDiscord>,
    kick: Option<FakeKick>,
    youtube: Option<FakeYouTube>,
    obs: Option<FakeObs>,
    vtube: Option<FakeVTube>,
    donatello: Option<FakeDonatello>,
    monobank: Option<FakeMonobank>,
    host: ForgeHost,
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
            event_triggers: Vec::new(),
        };
        let actions = ActionIndex::from_seed(&seed);
        let pages = OverlayPages::for_seed(&seed).unwrap();
        Self {
            forge,
            fake,
            discord: None,
            kick: None,
            youtube: None,
            obs: None,
            vtube: None,
            donatello: None,
            monobank: None,
            host: ForgeHost::attached(client, journal.clone()),
            journal,
            actions,
            pages,
            ping,
            logs: tempfile::tempdir().unwrap(),
        }
    }

    fn session(&self) -> Session<'_> {
        Session {
            forge: &self.host,
            journal: &self.journal,
            twitch: self.fake.as_ref(),
            discord: self.discord.as_ref(),
            kick: self.kick.as_ref(),
            youtube: self.youtube.as_ref(),
            obs: self.obs.as_ref(),
            vtube: self.vtube.as_ref(),
            donations: DonationFakes {
                donatello: self.donatello.as_ref(),
                monobank: self.monobank.as_ref().map(|fake| (fake, JAR)),
            },
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

fn unvalidated_scenario(steps: Value) -> Scenario {
    serde_json::from_value(json!({
        "name": "runner test",
        "purpose": "exercise step execution",
        "fixture": { "twitch": {}, "chat_commands": [{ "phrase": "!ping", "action_name": "Ping" }] },
        "fakes": { "twitch": {} },
        "steps": steps,
    }))
    .unwrap()
}

fn awaiting(subscription_type: &str) -> Value {
    json!({ "do": { "twitch_subscribed": { "types": [subscription_type], "within_ms": 5000 } } })
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
            "expect": [ { "event": { "kind": "chat.message", "count": { "at_least": 10 }, "within_ms": 15000 } } ]
        }
    ]));

    let steps = harness.run(&scenario).await;

    let delivered = harness.journal.read(|view| {
        view.entries
            .iter()
            .filter(|entry| matches!(&entry.observation, Observation::Event(event) if event.kind == "chat.message"))
            .count()
    });
    assert_eq!(
        (verdicts(&steps[2]), delivered),
        (vec![Verdict::Passed], 10),
        "{:#?}",
        steps[2]
    );
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
    harness
        .journal
        .wait_until(Instant::now() + DEADLINE, |view| !view.entries.is_empty())
        .await;

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
            "expect": [ { "event": { "kind": "custom.never", "within_ms": 60000 } } ]
        }]),
    );
    harness.forge.pushes.send(Value::Null).unwrap();

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[0]),
        [Verdict::Failed(FailureCause::StreamClosed)]
    );
}

#[tokio::test]
async fn twitch_event_delivers_its_notification_to_the_session_subscribed_to_that_type() {
    let harness = Harness::start(COOPERATIVE, true).await;
    let scenario = twitch_scenario(json!([
        ready(),
        awaiting(SUBSCRIBE),
        {
            "do": { "twitch_event": { "subscription_type": SUBSCRIBE, "event": {
                "user_login": "luckyviewer",
                "user_name": "LuckyViewer",
                "tier": "1000"
            } } },
            "expect": [{ "event": {
                "source": "twitch",
                "kind": "twitch.channel.subscribe",
                "payload": { "/user/display_name": { "equals": "LuckyViewer" } },
                "within_ms": 5000
            } }]
        }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(statuses(&steps), [StepStatus::Passed; 3]);
    assert!(
        matches!(
            steps[2].action.as_ref(),
            Some(ActionReport::Done(ActionDetail::TwitchEventDelivered {
                subscription_type,
                sessions,
            })) if subscription_type == SUBSCRIBE && *sessions == 1
        ),
        "got {:?}",
        steps[2].action
    );
}

#[tokio::test]
async fn twitch_event_no_session_holds_fails_the_step_naming_the_live_subscriptions() {
    let harness = Harness::start(COOPERATIVE, true).await;
    let scenario = unvalidated_scenario(json!([
        ready(),
        awaiting(SUBSCRIBE),
        { "do": { "twitch_event": { "subscription_type": "channel.raid", "event": {} } } }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        statuses(&steps),
        [StepStatus::Passed, StepStatus::Passed, StepStatus::Failed]
    );
    let Some(ActionReport::Failed { reason, .. }) = steps[2].action.as_ref() else {
        panic!("expected a failed action, got {:?}", steps[2].action);
    };
    assert!(
        reason.contains("`channel.raid`")
            && reason.contains(&format!("live subscriptions: {CHAT}, {SUBSCRIBE}")),
        "{reason}"
    );
}

const GO_LIVE: &str = "go-live";

fn discord_scenario(steps: Value) -> Scenario {
    scenario(
        json!({ "discord_webhooks": [{ "name": GO_LIVE }] }),
        json!({ "discord": {} }),
        steps,
    )
}

async fn harness_with_discord() -> (Harness, String) {
    let mut harness = Harness::start(COOPERATIVE, false).await;
    let discord = FakeDiscord::start(&[GO_LIVE.to_owned()]).await.unwrap();
    let url = discord.webhook_url(GO_LIVE).unwrap();
    harness.discord = Some(discord);
    (harness, url)
}

async fn post_to_webhook(url: &str, body: Value) {
    let status = reqwest::Client::new()
        .post(format!("{url}?wait=true"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "{status}");
}

fn role_ping_post(within_ms: u64) -> Value {
    json!({ "discord_post": {
        "webhook": GO_LIVE,
        "content_contains": "<@&42>",
        "mention_parse": ["roles", "users"],
        "within_ms": within_ms
    } })
}

#[tokio::test]
async fn discord_post_passes_on_a_post_with_the_expected_mention_parse_in_any_order() {
    let (harness, url) = harness_with_discord().await;
    post_to_webhook(
        &url,
        json!({ "content": "<@&42> we are live", "allowed_mentions": { "parse": ["users", "roles"] } }),
    )
    .await;
    let scenario = discord_scenario(json!([
        { "do": { "forge_ready": { "within_ms": 1000 } }, "expect": [role_ping_post(1000)] }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(verdicts(&steps[0]), [Verdict::Passed], "{steps:#?}");
}

#[tokio::test]
async fn discord_post_with_a_different_mention_parse_fails_and_shows_what_arrived() {
    let (harness, url) = harness_with_discord().await;
    post_to_webhook(
        &url,
        json!({ "content": "<@&42> we are live", "allowed_mentions": { "parse": ["users"] } }),
    )
    .await;
    let scenario = discord_scenario(json!([
        { "do": { "forge_ready": { "within_ms": 1000 } }, "expect": [role_ping_post(50)] }
    ]));

    let steps = harness.run(&scenario).await;

    let expectation = &steps[0].expectations[0];
    assert_eq!(
        expectation.verdict,
        Verdict::Failed(FailureCause::NoDiscordPost { observed: 1 })
    );
    assert!(
        matches!(&expectation.evidence, forge_emulator::run::Evidence::Discord(posts)
            if posts[0].mention_parse() == Some(vec!["users".to_owned()])),
        "{:?}",
        expectation.evidence
    );
}

#[tokio::test]
async fn a_discord_post_made_before_a_step_starts_never_satisfies_its_expectations() {
    let (harness, url) = harness_with_discord().await;
    post_to_webhook(
        &url,
        json!({ "content": "<@&42> we are live", "allowed_mentions": { "parse": ["users", "roles"] } }),
    )
    .await;
    let scenario = discord_scenario(json!([
        ready(),
        { "do": { "pause": { "ms": 1, "reason": "nothing posts now" } }, "expect": [role_ping_post(50)] }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[1]),
        [Verdict::Failed(FailureCause::NoDiscordPost { observed: 0 })]
    );
}

#[tokio::test]
async fn discord_post_without_a_fake_discord_fails_as_unrunnable() {
    let harness = Harness::start(COOPERATIVE, false).await;
    let scenario = discord_scenario(json!([
        { "do": { "forge_ready": { "within_ms": 1000 } }, "expect": [role_ping_post(50)] }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[0]),
        [Verdict::Failed(FailureCause::NoFakeDiscord)]
    );
}

const OBS_PASSWORD: &str = "obs-runner-secret";

struct QuietPublisher;

impl forge_events::EventPublisher for QuietPublisher {
    fn publish(&self, _: Event) {}
}

fn obs_scenario(online_at_boot: bool, steps: Value) -> Scenario {
    scenario(
        json!({ "obs": { "password": OBS_PASSWORD } }),
        json!({ "obs": { "password": OBS_PASSWORD, "online_at_boot": online_at_boot } }),
        steps,
    )
}

async fn harness_with_obs(online_at_boot: bool) -> Harness {
    let mut harness = Harness::start(COOPERATIVE, false).await;
    let obs = FakeObs::start(FakeObsConfig {
        password: Some(OBS_PASSWORD.to_owned()),
        online_at_boot,
        ..FakeObsConfig::default()
    })
    .await
    .unwrap();
    harness.obs = Some(obs);
    harness
}

async fn forge_obs_client(harness: &Harness, password: &str) -> forge_obs::ObsClient {
    let url = harness.obs.as_ref().unwrap().url();
    forge_obs::ObsClient::connect(&url, Some(password), std::sync::Arc::new(QuietPublisher))
        .await
        .unwrap()
}

#[tokio::test]
async fn obs_online_lets_a_retrying_forge_identify_and_read_the_scene_list() {
    let harness = harness_with_obs(false).await;
    let _forge = forge_obs_client(&harness, OBS_PASSWORD).await;
    let scenario = obs_scenario(
        false,
        json!([
            ready(),
            {
                "do": { "obs_online": {} },
                "expect": [
                    { "obs_auth": { "accepted": true, "within_ms": 10000 } },
                    { "obs_request": { "request_type": "GetSceneList", "code": 100, "within_ms": 10000 } }
                ]
            },
            { "do": { "obs_identified": { "within_ms": 1000 } } }
        ]),
    );

    let steps = harness.run(&scenario).await;

    assert_eq!(
        statuses(&steps),
        [StepStatus::Passed, StepStatus::Passed, StepStatus::Passed],
        "{steps:#?}"
    );
}

#[tokio::test]
async fn obs_restart_is_followed_by_forge_identifying_again() {
    let harness = harness_with_obs(true).await;
    let _forge = forge_obs_client(&harness, OBS_PASSWORD).await;
    let scenario = obs_scenario(
        true,
        json!([
            ready(),
            { "do": { "obs_identified": { "within_ms": 5000 } } },
            {
                "do": { "obs_restart": { "down_ms": 100 } },
                "expect": [{ "obs_auth": { "accepted": true, "within_ms": 10000 } }]
            }
        ]),
    );

    let steps = harness.run(&scenario).await;

    assert_eq!(
        statuses(&steps),
        [StepStatus::Passed, StepStatus::Passed, StepStatus::Passed],
        "{steps:#?}"
    );
    assert!(
        matches!(
            &steps[2].action,
            Some(ActionReport::Done(ActionDetail::ObsRestarted {
                closed_sessions: 1
            }))
        ),
        "{:?}",
        steps[2].action
    );
}

#[tokio::test]
async fn obs_auth_rejected_passes_when_forge_offers_the_wrong_password() {
    let harness = harness_with_obs(false).await;
    let _forge = forge_obs_client(&harness, "not-the-password").await;
    let scenario = obs_scenario(
        false,
        json!([
            ready(),
            {
                "do": { "obs_online": {} },
                "expect": [{ "obs_auth": { "accepted": false, "within_ms": 10000 } }]
            }
        ]),
    );

    let steps = harness.run(&scenario).await;

    assert_eq!(verdicts(&steps[1]), [Verdict::Passed], "{steps:#?}");
}

#[tokio::test]
async fn an_obs_studio_change_no_session_receives_fails_its_step() {
    let harness = harness_with_obs(true).await;
    let scenario = obs_scenario(
        true,
        json!([ready(), { "do": { "obs_scene_switch": { "scene": "BRB" } } }]),
    );

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(&steps[1].action, Some(ActionReport::Failed { reason, .. })
            if reason.contains("no identified OBS session")),
        "{:?}",
        steps[1].action
    );
}

#[tokio::test]
async fn an_obs_request_made_before_a_step_starts_never_satisfies_its_expectations() {
    let harness = harness_with_obs(true).await;
    let _forge = forge_obs_client(&harness, OBS_PASSWORD).await;
    harness
        .obs
        .as_ref()
        .unwrap()
        .wait_for("the catalog load", DEADLINE, |ledger| {
            ledger
                .requests
                .iter()
                .any(|request| request.request_type == "GetSceneList")
                .then_some(())
        })
        .await
        .unwrap();
    let scenario = obs_scenario(
        true,
        json!([
            ready(),
            {
                "do": { "pause": { "ms": 1, "reason": "forge already loaded its catalog" } },
                "expect": [{ "obs_request": { "request_type": "GetSceneList", "within_ms": 50 } }]
            }
        ]),
    );

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(
            verdicts(&steps[1]).as_slice(),
            [Verdict::Failed(FailureCause::NoObsRequest { .. })]
        ),
        "{steps:#?}"
    );
}

#[tokio::test]
async fn obs_checks_without_a_fake_obs_fail_as_unrunnable() {
    let harness = Harness::start(COOPERATIVE, false).await;
    let scenario = obs_scenario(
        true,
        json!([{
            "do": { "forge_ready": { "within_ms": 1000 } },
            "expect": [{ "obs_auth": { "accepted": true, "within_ms": 50 } }]
        }]),
    );

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[0]),
        [Verdict::Failed(FailureCause::NoFakeObs)]
    );
}

const JAR: &str = "jar-stream";

async fn harness_with_donations() -> Harness {
    let mut harness = Harness::start(COOPERATIVE, false).await;
    harness.donatello = Some(
        FakeDonatello::start(FakeDonatelloConfig::default())
            .await
            .unwrap(),
    );
    harness.monobank = Some(
        FakeMonobank::start(FakeMonobankConfig {
            jars: vec![FakeJar::new(JAR, "Stream jar")],
            ..FakeMonobankConfig::default()
        })
        .await
        .unwrap(),
    );
    harness
}

fn donation_scenario(steps: Value) -> Scenario {
    scenario(
        json!({ "donatello": {}, "monobank": { "jar_id": JAR } }),
        json!({ "donatello": {}, "monobank": {} }),
        steps,
    )
}

async fn donatello_list(harness: &Harness) -> Response {
    let fake = harness.donatello.as_ref().unwrap();
    reqwest::Client::new()
        .get(format!("{}/donates?page=0&size=20", fake.base_url()))
        .header("X-Token", FAKE_DONATELLO_TOKEN)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_donatello_donation_step_lists_the_gift_on_the_next_read() {
    let harness = harness_with_donations().await;
    let scenario = donation_scenario(json!([
        ready(),
        { "do": { "donatello_donation": { "id": "dl-1", "donor": "Olena", "amount": "150" } } }
    ]));

    harness.run(&scenario).await;

    let page: Value = donatello_list(&harness).await.json().await.unwrap();
    assert_eq!(page["content"][0]["pubId"], json!("dl-1"), "{page}");
}

#[tokio::test]
async fn a_monobank_top_up_step_lands_in_the_seeded_jar() {
    let harness = harness_with_donations().await;
    let scenario = donation_scenario(json!([
        ready(),
        { "do": { "monobank_top_up": { "id": "ml-1", "sender": "Taras", "amount_minor": 2500 } } }
    ]));
    let from = OffsetDateTime::now_utc().unix_timestamp() - 60;

    harness.run(&scenario).await;

    let fake = harness.monobank.as_ref().unwrap();
    let to = OffsetDateTime::now_utc().unix_timestamp() + 60;
    let items: Value = reqwest::Client::new()
        .get(format!(
            "{}/personal/statement/{JAR}/{from}/{to}",
            fake.base_url()
        ))
        .header("X-Token", FAKE_MONOBANK_TOKEN)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(items[0]["id"], json!("ml-1"), "{items}");
}

fn donatello_only_scenario(steps: Value) -> Scenario {
    scenario(
        json!({ "donatello": {} }),
        json!({ "donatello": {} }),
        steps,
    )
}

#[tokio::test]
async fn donations_polled_fails_when_no_list_is_read_after_the_step_begins() {
    let mut harness = harness_with_donations().await;
    harness.monobank = None;
    donatello_list(&harness).await;
    let scenario = donatello_only_scenario(json!([
        ready(),
        { "do": { "donations_polled": { "within_ms": 200 } } }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(statuses(&steps), [StepStatus::Passed, StepStatus::Failed]);
}

#[tokio::test]
async fn donations_polled_passes_once_every_service_serves_a_list() {
    let mut harness = harness_with_donations().await;
    harness.monobank = None;
    let base_url = harness.donatello.as_ref().unwrap().base_url().to_owned();
    let poller = tokio::spawn(async move {
        loop {
            let _ = reqwest::Client::new()
                .get(format!("{base_url}/donates?page=0&size=20"))
                .header("X-Token", FAKE_DONATELLO_TOKEN)
                .send()
                .await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    let scenario = donatello_only_scenario(json!([
        ready(),
        { "do": { "donations_polled": { "within_ms": 5000 } } }
    ]));

    let steps = harness.run(&scenario).await;
    poller.abort();

    assert_eq!(statuses(&steps), [StepStatus::Passed, StepStatus::Passed]);
}

#[tokio::test]
async fn forge_restart_fails_on_a_forge_the_emulator_did_not_launch() {
    let harness = harness_with_donations().await;
    let scenario = donation_scenario(json!([
        ready(),
        { "do": { "forge_restart": { "within_ms": 1000 } } }
    ]));

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(
            &steps[1].action,
            Some(ActionReport::Failed { reason, .. }) if reason.contains("cannot be restarted")
        ),
        "{:?}",
        steps[1].action
    );
}

const VTUBE_TOKEN: &str = "vtube-runner-token";

#[derive(Default)]
struct MemoryCredentials(std::sync::Mutex<std::collections::HashMap<String, String>>);

#[async_trait::async_trait]
impl forge_storage::CredentialsRepo for MemoryCredentials {
    async fn store(
        &self,
        id: &forge_storage::CredentialId,
        plaintext: &str,
    ) -> Result<(), forge_storage::StorageError> {
        self.0
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), plaintext.to_owned());
        Ok(())
    }

    async fn load(
        &self,
        id: &forge_storage::CredentialId,
    ) -> Result<Option<String>, forge_storage::StorageError> {
        Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn delete(
        &self,
        id: &forge_storage::CredentialId,
    ) -> Result<bool, forge_storage::StorageError> {
        Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
    }

    async fn list_ids(
        &self,
    ) -> Result<Vec<forge_storage::CredentialId>, forge_storage::StorageError> {
        Ok(Vec::new())
    }

    async fn last_refresh(
        &self,
        _: &forge_storage::CredentialId,
    ) -> Result<Option<time::OffsetDateTime>, forge_storage::StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(
        &self,
        _: &forge_storage::CredentialId,
    ) -> Result<(), forge_storage::StorageError> {
        Ok(())
    }
}

fn vtube_scenario(online_at_boot: bool, steps: Value) -> Scenario {
    scenario(
        json!({ "vtube": { "token": VTUBE_TOKEN } }),
        json!({ "vtube": { "token": VTUBE_TOKEN, "online_at_boot": online_at_boot } }),
        steps,
    )
}

async fn harness_with_vtube(online_at_boot: bool) -> Harness {
    let mut harness = Harness::start(COOPERATIVE, false).await;
    let vtube = FakeVTube::start(FakeVTubeConfig {
        token: VTUBE_TOKEN.to_owned(),
        online_at_boot,
        ..FakeVTubeConfig::default()
    })
    .await
    .unwrap();
    harness.vtube = Some(vtube);
    harness
}

async fn forge_vtube_client(harness: &Harness, token: &str) -> Arc<forge_vtube::VTubeClient> {
    let port = harness.vtube.as_ref().unwrap().port();
    let credentials = Arc::new(MemoryCredentials::default());
    forge_vtube::credentials::store(&*credentials, token, "1.0", "127.0.0.1", port)
        .await
        .unwrap();
    forge_vtube::credentials::load_and_connect(
        &*credentials,
        Arc::new(QuietPublisher),
        credentials.clone(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn vtube_online_lets_a_retrying_forge_authenticate_and_subscribe() {
    let harness = harness_with_vtube(false).await;
    let forge = forge_vtube_client(&harness, VTUBE_TOKEN).await;
    let scenario = vtube_scenario(
        false,
        json!([
            ready(),
            {
                "do": { "vtube_online": {} },
                "expect": [
                    { "vtube_auth": { "accepted": true, "within_ms": 10000 } },
                    {
                        "vtube_request": {
                            "message_type": "EventSubscriptionRequest",
                            "data": { "/eventName": { "equals": "ItemEvent" } },
                            "succeeded": true,
                            "within_ms": 10000
                        }
                    }
                ]
            },
            { "do": { "vtube_authenticated": { "within_ms": 1000 } } }
        ]),
    );

    let steps = harness.run(&scenario).await;
    forge.shutdown().await;

    assert_eq!(
        statuses(&steps),
        [StepStatus::Passed, StepStatus::Passed, StepStatus::Passed],
        "{steps:#?}"
    );
}

#[tokio::test]
async fn vtube_auth_rejected_passes_when_forge_offers_a_revoked_token() {
    let harness = harness_with_vtube(false).await;
    let forge = forge_vtube_client(&harness, "a-revoked-token").await;
    let scenario = vtube_scenario(
        false,
        json!([
            ready(),
            {
                "do": { "vtube_online": {} },
                "expect": [
                    { "vtube_auth": { "accepted": false, "within_ms": 10000 } },
                    { "vtube_auth": { "accepted": true, "within_ms": 50 } }
                ]
            }
        ]),
    );

    let steps = harness.run(&scenario).await;
    forge.shutdown().await;

    assert_eq!(
        verdicts(&steps[1]),
        [
            Verdict::Passed,
            Verdict::Failed(FailureCause::NoVTubeAuth { sessions: 1 })
        ],
        "{steps:#?}"
    );
}

#[tokio::test]
async fn vtube_request_expectations_tell_answered_requests_from_refused_ones() {
    let harness = harness_with_vtube(true).await;
    let forge = forge_vtube_client(&harness, VTUBE_TOKEN).await;
    let poll = |within_ms: u64, answer: Value| {
        let mut request =
            json!({ "message_type": "ExpressionStateRequest", "within_ms": within_ms });
        request
            .as_object_mut()
            .unwrap()
            .extend(answer.as_object().unwrap().clone());
        json!({ "vtube_request": request })
    };
    let scenario = vtube_scenario(
        true,
        json!([
            ready(),
            { "do": { "vtube_authenticated": { "within_ms": 10000 } } },
            {
                "do": { "pause": { "ms": 10, "reason": "let the expression poll run" } },
                "expect": [poll(15000, json!({ "succeeded": true })), poll(50, json!({ "error_id": 601 }))]
            }
        ]),
    );

    let steps = harness.run(&scenario).await;
    forge.shutdown().await;

    assert!(
        matches!(
            verdicts(&steps[2]).as_slice(),
            [
                Verdict::Passed,
                Verdict::Failed(FailureCause::NoVTubeRequest { .. })
            ]
        ),
        "{steps:#?}"
    );
}

#[tokio::test]
async fn a_vtube_studio_change_no_session_receives_fails_its_step() {
    let harness = harness_with_vtube(true).await;
    let scenario = vtube_scenario(
        true,
        json!([ready(), { "do": { "vtube_hotkey": { "hotkey": "Wave" } } }]),
    );

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(&steps[1].action, Some(ActionReport::Failed { reason, .. })
            if reason.contains("no authenticated VTube Studio session")),
        "{:?}",
        steps[1].action
    );
}

#[tokio::test]
async fn a_vtube_expression_change_passes_without_a_listening_session() {
    let harness = harness_with_vtube(true).await;
    let scenario = vtube_scenario(
        true,
        json!([
            ready(),
            { "do": { "vtube_expression": { "file": "Blush.exp3.json", "active": true } } }
        ]),
    );

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(
            steps[1].action,
            Some(ActionReport::Done(ActionDetail::VTubeStateChanged))
        ),
        "{steps:#?}"
    );
}

fn kick_scenario(steps: Value) -> Scenario {
    scenario(json!({ "kick": {} }), json!({ "kick": {} }), steps)
}

async fn harness_with_kick() -> Harness {
    let mut harness = Harness::start(COOPERATIVE, false).await;
    harness.kick = Some(
        FakeKick::start(forge_emulator::kick::FakeKickConfig::default())
            .await
            .unwrap(),
    );
    harness
}

fn kick_endpoints(fake: &FakeKick) -> forge_platform_core::PlatformEndpoints {
    let overrides = fake.endpoint_overrides();
    forge_platform_core::PlatformEndpoints::resolve(|variable| {
        overrides
            .iter()
            .find(|(name, _)| *name == variable)
            .map(|(_, url)| std::ffi::OsString::from(url))
    })
    .unwrap()
}

struct KickGrant;

#[async_trait::async_trait]
impl forge_platform_core::RateLimiter for KickGrant {
    async fn acquire(
        &self,
        _weight: u32,
    ) -> Result<forge_platform_core::RateLimitOutcome, forge_platform_core::PlatformError> {
        Ok(forge_platform_core::RateLimitOutcome::Granted)
    }

    async fn observe_remote_throttle(&self, _retry_after: Duration) {}
}

async fn forge_kick_echo(fake: &FakeKick) -> forge_platform_kick::KickChatHandle {
    let endpoints = kick_endpoints(fake);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<Event>(16);
    let handle = forge_platform_kick::KickChat::new(
        &endpoints,
        fake.config().username.clone(),
        reqwest::Client::new(),
    )
    .connect(event_tx)
    .await
    .unwrap();
    let sender = forge_platform_kick::KickSendChat::new(&endpoints, Arc::new(KickGrant));
    let token = fake.access_token();
    let broadcaster = fake.config().user_id;
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let text = event.payload["content"].as_str().unwrap_or_default();
            let _ = sender
                .send(&format!("echo {text}"), &token, broadcaster, false)
                .await;
        }
    });
    handle
}

#[tokio::test]
async fn kick_chat_reaches_a_joined_forge_and_its_reply_satisfies_kick_request() {
    let harness = harness_with_kick().await;
    let forge = forge_kick_echo(harness.kick.as_ref().unwrap()).await;
    let scenario = kick_scenario(json!([
        ready(),
        { "do": { "kick_chat_joined": { "within_ms": 10000 } } },
        {
            "do": { "kick_chat": { "sender": { "user_id": 7, "username": "alice" }, "text": "hi" } },
            "expect": [
                {
                    "kick_request": {
                        "method": "POST",
                        "path": "/public/v1/chat",
                        "body": { "/content": { "equals": "echo hi" } },
                        "status": 200,
                        "within_ms": 10000
                    }
                },
                { "kick_request_count": { "path": "/public/v1/chat", "min": 1, "max": 1 } },
                { "kick_no_unexpected_requests": {} }
            ]
        }
    ]));

    let steps = harness.run(&scenario).await;
    forge.shutdown();

    assert_eq!(
        verdicts(&steps[2]),
        [Verdict::Passed, Verdict::Passed, Verdict::Passed],
        "{steps:#?}"
    );
}

#[tokio::test]
async fn kick_request_fails_when_forge_sends_nothing_in_the_window() {
    let harness = harness_with_kick().await;
    let scenario = kick_scenario(json!([
        ready(),
        {
            "do": { "pause": { "ms": 10, "reason": "give forge a moment" } },
            "expect": [
                { "kick_request": { "path": "/public/v1/chat", "within_ms": 100 } }
            ]
        }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[1]),
        [Verdict::Failed(FailureCause::NoKickRequest { observed: 0 })],
        "{steps:#?}"
    );
}

#[tokio::test]
async fn kick_chat_fails_its_step_when_no_connection_joined() {
    let harness = harness_with_kick().await;
    let scenario = kick_scenario(json!([
        ready(),
        { "do": { "kick_chat": { "sender": { "user_id": 7, "username": "alice" }, "text": "hi" } } }
    ]));

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(&steps[1].action, Some(ActionReport::Failed { reason, .. })
            if reason.contains("no live Kick chat connection")),
        "{:?}",
        steps[1].action
    );
}

#[tokio::test]
async fn kick_no_unexpected_requests_fails_after_an_unmodeled_call() {
    let harness = harness_with_kick().await;
    let fake = harness.kick.as_ref().unwrap();
    let users = format!(
        "{}/users",
        kick_endpoints(fake).base_url(forge_platform_core::EndpointSurface::KickPublicApi)
    );
    reqwest::get(&users).await.unwrap();
    let scenario = kick_scenario(json!([
        ready(),
        {
            "do": { "pause": { "ms": 10, "reason": "give forge a moment" } },
            "expect": [{ "kick_no_unexpected_requests": {} }]
        }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[1]),
        [Verdict::Failed(FailureCause::UnexpectedKickRequests {
            count: 1
        })],
        "{steps:#?}"
    );
}

#[tokio::test]
async fn kick_channel_polled_passes_once_a_polling_forge_reads_the_channel() {
    let harness = harness_with_kick().await;
    let fake = harness.kick.as_ref().unwrap();
    let channel = forge_platform_kick::KickChannel::new(&kick_endpoints(fake), Arc::new(KickGrant));
    let token = fake.access_token();
    let poller = tokio::spawn(async move {
        loop {
            channel.get_channel(&token).await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    let scenario = kick_scenario(json!([
        ready(),
        { "do": { "kick_channel_polled": { "within_ms": 5000 } } }
    ]));

    let steps = harness.run(&scenario).await;
    poller.abort();

    assert_eq!(steps[1].status, StepStatus::Passed, "{steps:#?}");
}

#[tokio::test]
async fn kick_channel_polled_fails_when_forge_never_reads_the_channel() {
    let harness = harness_with_kick().await;
    let scenario = kick_scenario(json!([
        ready(),
        { "do": { "kick_channel_polled": { "within_ms": 100 } } }
    ]));

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(&steps[1].action, Some(ActionReport::Failed { reason, .. })
            if reason.contains("a Kick channel poll")),
        "{:?}",
        steps[1].action
    );
}

fn youtube_scenario(steps: Value) -> Scenario {
    scenario(
        json!({ "youtube": {} }),
        json!({ "youtube": { "live_at_boot": true } }),
        steps,
    )
}

async fn harness_with_youtube(live: bool) -> Harness {
    let mut harness = Harness::start(COOPERATIVE, false).await;
    harness.youtube = Some(
        FakeYouTube::start(FakeYouTubeConfig {
            live,
            ..FakeYouTubeConfig::default()
        })
        .await
        .unwrap(),
    );
    harness
}

fn youtube_endpoints(fake: &FakeYouTube) -> forge_platform_core::PlatformEndpoints {
    let overrides = fake.endpoint_overrides();
    forge_platform_core::PlatformEndpoints::resolve(|variable| {
        overrides
            .iter()
            .find(|(name, _)| *name == variable)
            .map(|(_, url)| std::ffi::OsString::from(url))
    })
    .unwrap()
}

fn youtube_token(
    fake: &FakeYouTube,
) -> Arc<
    dyn Fn() -> futures_util::future::BoxFuture<
            'static,
            Result<String, forge_platform_core::PlatformError>,
        > + Send
        + Sync,
> {
    let token = fake.access_token();
    Arc::new(move || {
        let token = token.clone();
        Box::pin(async move { Ok(token) })
    })
}

fn forge_youtube_echo(fake: &FakeYouTube) -> tokio::task::JoinHandle<()> {
    let endpoints = youtube_endpoints(fake);
    let live_chat = forge_platform_youtube::LiveChatIdHandle::new();
    let quota = forge_platform_youtube::SharedQuota::default();
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let poller = forge_platform_youtube::YoutubeChatPoller::new(
        &endpoints,
        youtube_token(fake),
        event_tx,
        fake.config().channel_id.clone(),
        live_chat.clone(),
        forge_platform_youtube::ActiveBroadcastIdHandle::new(),
        quota.clone(),
        Arc::new(forge_storage::ban_ledger::MockBanLedgerRepo::new()),
    );
    let sender = forge_platform_youtube::YoutubeSendChat::new(
        &endpoints,
        youtube_token(fake),
        live_chat,
        quota,
    );
    tokio::spawn(async move {
        let polling = tokio::spawn(poller.run(tokio_util::sync::CancellationToken::new()));
        while let Some(event) = event_rx.recv().await {
            if event.kind == "youtube.chat.command" {
                let text = event.payload["message_text"].as_str().unwrap_or_default();
                sender.send(&format!("echo {text}")).await.ok();
            }
        }
        polling.abort();
    })
}

#[tokio::test]
async fn youtube_chat_reaches_a_polling_forge_and_its_reply_satisfies_youtube_request() {
    let harness = harness_with_youtube(true).await;
    let forge = forge_youtube_echo(harness.youtube.as_ref().unwrap());
    let scenario = youtube_scenario(json!([
        ready(),
        { "do": { "youtube_chat_polled": { "within_ms": 10000 } } },
        {
            "do": {
                "youtube_chat": {
                    "author": { "channel_id": "UCaliceViewer00000000001", "display_name": "Alice" },
                    "text": "!hi"
                }
            },
            "expect": [
                {
                    "youtube_request": {
                        "method": "POST",
                        "path": "/youtube/v3/liveChat/messages",
                        "body": { "/snippet/textMessageDetails/messageText": { "equals": "echo !hi" } },
                        "status": 200,
                        "within_ms": 10000
                    }
                },
                { "youtube_request_count": { "method": "POST", "path": "/youtube/v3/liveChat/messages", "min": 1, "max": 1 } },
                { "youtube_no_unexpected_requests": {} }
            ]
        }
    ]));

    let steps = harness.run(&scenario).await;
    forge.abort();

    assert_eq!(
        verdicts(&steps[2]),
        [Verdict::Passed, Verdict::Passed, Verdict::Passed],
        "{steps:#?}"
    );
}

#[tokio::test]
async fn youtube_request_fails_when_forge_sends_nothing_in_the_window() {
    let harness = harness_with_youtube(true).await;
    let scenario = youtube_scenario(json!([
        ready(),
        {
            "do": { "pause": { "ms": 10, "reason": "give forge a moment" } },
            "expect": [
                { "youtube_request": { "path": "/youtube/v3/liveChat/messages", "within_ms": 100 } }
            ]
        }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[1]),
        [Verdict::Failed(FailureCause::NoYouTubeRequest {
            observed: 0
        })],
        "{steps:#?}"
    );
}

#[tokio::test]
async fn youtube_chat_polled_fails_naming_the_build_time_client_when_forge_never_polls() {
    let harness = harness_with_youtube(true).await;
    let scenario = youtube_scenario(json!([
        ready(),
        { "do": { "youtube_chat_polled": { "within_ms": 100 } } }
    ]));

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(&steps[1].action, Some(ActionReport::Failed { reason, .. })
            if reason.contains("FORGE_YOUTUBE_CLIENT_ID")),
        "{:?}",
        steps[1].action
    );
}

#[tokio::test]
async fn youtube_chat_fails_its_step_while_the_broadcast_is_offline() {
    let harness = harness_with_youtube(false).await;
    let scenario = youtube_scenario(json!([
        ready(),
        {
            "do": {
                "youtube_chat": {
                    "author": { "channel_id": "UCaliceViewer00000000001", "display_name": "Alice" },
                    "text": "hi"
                }
            }
        }
    ]));

    let steps = harness.run(&scenario).await;

    assert!(
        matches!(&steps[1].action, Some(ActionReport::Failed { reason, .. })
            if reason.contains("is not live")),
        "{:?}",
        steps[1].action
    );
}

#[tokio::test]
async fn youtube_no_unexpected_requests_fails_after_an_unmodeled_call() {
    let harness = harness_with_youtube(true).await;
    let fake = harness.youtube.as_ref().unwrap();
    reqwest::Client::new()
        .put(format!(
            "{}/videos",
            youtube_endpoints(fake).base_url(forge_platform_core::EndpointSurface::YouTubeDataApi)
        ))
        .bearer_auth(fake.access_token())
        .send()
        .await
        .unwrap();
    let scenario = youtube_scenario(json!([
        ready(),
        {
            "do": { "pause": { "ms": 10, "reason": "give forge a moment" } },
            "expect": [{ "youtube_no_unexpected_requests": {} }]
        }
    ]));

    let steps = harness.run(&scenario).await;

    assert_eq!(
        verdicts(&steps[1]),
        [Verdict::Failed(FailureCause::UnexpectedYouTubeRequests {
            count: 1
        })],
        "{steps:#?}"
    );
}
