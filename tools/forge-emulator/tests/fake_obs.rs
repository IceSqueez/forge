#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use forge_emulator::EmulatorError;
use forge_emulator::obs::{
    Authentication, FakeInput, FakeObs, FakeObsConfig, ObsLedger, authentication_string,
};
use forge_events::{Event, EventPublisher};
use forge_obs::{ObsClient, ObsError, ObsSink, probe_connection};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const PASSWORD: &str = "obs-fake-secret-7c1d";
const WAIT: Duration = Duration::from_secs(10);
const LOCALHOST: &str = "127.0.0.1";

#[derive(Default)]
struct Captured(Mutex<Vec<Event>>);

impl EventPublisher for Captured {
    fn publish(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl Captured {
    fn kinds(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.kind.clone())
            .collect()
    }
}

fn with_password() -> FakeObsConfig {
    FakeObsConfig {
        password: Some(PASSWORD.to_owned()),
        ..FakeObsConfig::default()
    }
}

async fn open(fake: &FakeObs) -> (Socket, Value) {
    let (mut socket, _) = tokio_tungstenite::connect_async(fake.url()).await.unwrap();
    let hello = next_json(&mut socket).await.expect("a Hello frame");
    (socket, hello)
}

async fn next_json(socket: &mut Socket) -> Option<Value> {
    loop {
        match timeout(WAIT, socket.next()).await.ok()?? {
            Ok(Message::Text(text)) => return serde_json::from_str(text.as_str()).ok(),
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(_) => {}
        }
    }
}

async fn close_code(socket: &mut Socket) -> Option<u16> {
    loop {
        match timeout(WAIT, socket.next()).await.ok()?? {
            Ok(Message::Close(frame)) => return frame.map(|frame| u16::from(frame.code)),
            Ok(_) => {}
            Err(_) => return None,
        }
    }
}

async fn send(socket: &mut Socket, value: Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

fn answer_to(hello: &Value, password: &str) -> Value {
    let auth = &hello["d"]["authentication"];
    json!(authentication_string(
        password,
        auth["salt"].as_str().unwrap(),
        auth["challenge"].as_str().unwrap()
    ))
}

async fn identified(fake: &FakeObs, subscriptions: Option<u64>) -> Socket {
    let (mut socket, hello) = open(fake).await;
    let mut data = json!({ "rpcVersion": 1, "authentication": answer_to(&hello, PASSWORD) });
    if let Some(subscriptions) = subscriptions {
        data["eventSubscriptions"] = json!(subscriptions);
    }
    send(&mut socket, json!({ "op": 1, "d": data })).await;
    let reply = next_json(&mut socket).await.unwrap();
    assert_eq!(reply["op"], 2, "{reply}");
    socket
}

async fn request(socket: &mut Socket, request_type: &str, data: Value) -> Value {
    send(
        socket,
        json!({ "op": 6, "d": { "requestType": request_type, "requestId": "r1", "requestData": data } }),
    )
    .await;
    loop {
        let frame = next_json(socket).await.unwrap();
        if frame["op"] == 7 {
            return frame["d"].clone();
        }
    }
}

async fn wait_ledger<T>(
    fake: &FakeObs,
    probe: impl FnMut(&ObsLedger) -> Option<T>,
) -> Result<T, EmulatorError> {
    fake.wait_for("ledger state", WAIT, probe).await
}

#[tokio::test]
async fn forge_probe_authenticates_and_reads_the_scene_list() {
    let fake = FakeObs::start(with_password()).await.unwrap();

    let probe = probe_connection(LOCALHOST, fake.port(), PASSWORD)
        .await
        .unwrap();

    assert_eq!(probe.scene_count, 2);
    let session = &fake.ledger().sessions[0];
    assert_eq!(session.authentication, Authentication::Accepted);
}

#[tokio::test]
async fn forge_probe_with_the_wrong_password_is_refused_as_an_authentication_failure() {
    let fake = FakeObs::start(with_password()).await.unwrap();

    let outcome = probe_connection(LOCALHOST, fake.port(), "not-the-password")
        .await
        .map(|probe| probe.scene_count);

    assert!(
        matches!(outcome, Err(ObsError::Authentication)),
        "{outcome:?}"
    );
    let session = wait_ledger(&fake, |ledger| {
        ledger.sessions.first().filter(|s| s.closed).cloned()
    })
    .await
    .unwrap();
    assert_eq!(
        (session.authentication, session.close_code),
        (Authentication::Rejected, Some(4009))
    );
}

#[tokio::test]
async fn forge_client_primes_its_catalog_and_publishes_connected() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let captured = Arc::new(Captured::default());

    let _client = ObsClient::connect(&fake.url(), Some(PASSWORD), captured.clone())
        .await
        .unwrap();

    wait_ledger(&fake, |ledger| {
        ledger
            .requests
            .iter()
            .any(|r| r.request_type == "GetRecordStatus")
            .then_some(())
    })
    .await
    .unwrap();
    timeout(WAIT, async {
        while !captured
            .kinds()
            .contains(&"obs.connection.connected".to_owned())
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("forge's client reported the connection");
    let unanswered: Vec<_> = fake
        .ledger()
        .requests
        .into_iter()
        .filter(|r| r.code == 204)
        .collect();
    assert!(unanswered.is_empty(), "{unanswered:?}");
}

#[tokio::test]
async fn forge_scene_switch_is_recorded_and_echoed_as_a_scene_changed_event() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let captured = Arc::new(Captured::default());
    let client = ObsClient::connect(&fake.url(), Some(PASSWORD), captured.clone())
        .await
        .unwrap();
    wait_ledger(&fake, |ledger| ledger.live_sessions().next().map(|_| ()))
        .await
        .unwrap();

    timeout(WAIT, async {
        while client.set_scene("BRB").await.is_err() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(fake.current_scene().as_deref(), Some("BRB"));
    timeout(WAIT, async {
        while !captured.kinds().contains(&"obs.scene.changed".to_owned()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("forge's client saw the scene change");
}

#[tokio::test]
async fn protocol_violations_close_the_connection_with_the_spec_close_code() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    for (label, identify_first, frame, expected) in [
        (
            "request before identify",
            false,
            json!({ "op": 6, "d": { "requestType": "GetVersion", "requestId": "1" } }),
            4007,
        ),
        (
            "unsupported rpc version",
            false,
            json!({ "op": 1, "d": { "rpcVersion": 2 } }),
            4010,
        ),
        (
            "identify without the authentication string",
            false,
            json!({ "op": 1, "d": { "rpcVersion": 1 } }),
            4009,
        ),
        (
            "frame without op",
            false,
            json!({ "d": { "rpcVersion": 1 } }),
            4003,
        ),
        (
            "second identify",
            true,
            json!({ "op": 1, "d": { "rpcVersion": 1 } }),
            4008,
        ),
        ("unknown op code", true, json!({ "op": 42, "d": {} }), 4006),
    ] {
        let mut socket = if identify_first {
            identified(&fake, None).await
        } else {
            open(&fake).await.0
        };
        send(&mut socket, frame).await;
        assert_eq!(close_code(&mut socket).await, Some(expected), "{label}");
    }
}

#[tokio::test]
async fn a_frame_that_is_not_json_closes_with_message_decode_error() {
    let fake = FakeObs::start(FakeObsConfig::default()).await.unwrap();
    let (mut socket, _) = open(&fake).await;

    socket
        .send(Message::Text("{not json".into()))
        .await
        .unwrap();

    assert_eq!(close_code(&mut socket).await, Some(4002));
}

#[tokio::test]
async fn hello_carries_an_auth_challenge_only_when_a_password_is_set() {
    let open_fake = FakeObs::start(FakeObsConfig::default()).await.unwrap();
    let guarded = FakeObs::start(with_password()).await.unwrap();

    let (_, open_hello) = open(&open_fake).await;
    let (_, guarded_hello) = open(&guarded).await;

    assert_eq!(
        (
            open_hello["d"].get("authentication").is_some(),
            guarded_hello["d"]["authentication"]["challenge"].is_string(),
            guarded_hello["d"]["rpcVersion"].clone(),
        ),
        (false, true, json!(1))
    );
}

#[tokio::test]
async fn failing_requests_answer_with_the_spec_status_code() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let mut socket = identified(&fake, None).await;
    for (request_type, data, expected) in [
        (
            "SetCurrentProgramScene",
            json!({ "sceneName": "Nope" }),
            600,
        ),
        ("SetCurrentProgramScene", json!({}), 300),
        ("SetInputMute", json!({ "inputName": "Mic/Aux" }), 300),
        ("GetInputVolume", json!({ "inputName": "Nope" }), 600),
        (
            "GetSceneItemEnabled",
            json!({ "sceneName": "Main", "sceneItemId": 99 }),
            600,
        ),
        ("GetCurrentPreviewScene", json!({}), 506),
        ("StopStream", json!({}), 501),
        ("TriggerHotkeyByName", json!({}), 204),
    ] {
        let response = request(&mut socket, request_type, data).await;
        assert_eq!(
            (
                response["requestStatus"]["result"].clone(),
                response["requestStatus"]["code"].clone()
            ),
            (json!(false), json!(expected)),
            "{request_type}"
        );
    }
}

#[tokio::test]
async fn starting_a_running_stream_is_refused_as_output_running() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let mut socket = identified(&fake, None).await;
    let first = request(&mut socket, "StartStream", json!({})).await;

    let second = request(&mut socket, "StartStream", json!({})).await;

    assert_eq!(
        (
            first["requestStatus"]["code"].clone(),
            second["requestStatus"]["code"].clone()
        ),
        (json!(100), json!(500))
    );
}

#[tokio::test]
async fn events_reach_only_sessions_subscribed_to_their_category() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    const OUTPUTS: u64 = 1 << 6;
    let mut outputs_only = identified(&fake, Some(OUTPUTS)).await;

    fake.switch_scene("BRB").unwrap();
    fake.set_streaming(true).unwrap();

    let first = next_json(&mut outputs_only).await.unwrap();
    assert_eq!(
        (
            first["d"]["eventType"].clone(),
            first["d"]["eventIntent"].clone()
        ),
        (json!("StreamStateChanged"), json!(OUTPUTS))
    );
}

#[tokio::test]
async fn starting_the_stream_pushes_starting_then_started() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let mut socket = identified(&fake, None).await;

    fake.set_streaming(true).unwrap();

    let mut states = Vec::new();
    for _ in 0..2 {
        let frame = next_json(&mut socket).await.unwrap();
        states.push(frame["d"]["eventData"]["outputState"].clone());
    }
    assert_eq!(
        states,
        [
            json!("OBS_WEBSOCKET_OUTPUT_STARTING"),
            json!("OBS_WEBSOCKET_OUTPUT_STARTED")
        ]
    );
}

#[tokio::test]
async fn muting_an_input_pushes_its_new_state_and_repeating_it_pushes_nothing() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let _socket = identified(&fake, None).await;

    let first = fake.set_input_mute("Mic/Aux", true).unwrap();
    let repeated = fake.set_input_mute("Mic/Aux", true).unwrap();

    assert_eq!(
        (first, repeated, fake.input_muted("Mic/Aux")),
        (1, 0, Some(true))
    );
}

#[tokio::test]
async fn studio_changes_naming_unknown_scenes_or_inputs_are_refused() {
    let fake = FakeObs::start(FakeObsConfig::default()).await.unwrap();

    let scene = fake.switch_scene("Nope");
    let input = fake.set_input_mute("Nope", true);

    assert!(
        matches!(
            (&scene, &input),
            (
                Err(EmulatorError::FakeObsRefused { .. }),
                Err(EmulatorError::FakeObsRefused { .. })
            )
        ),
        "{scene:?} {input:?}"
    );
}

#[tokio::test]
async fn restart_drops_live_sessions_going_away_and_accepts_new_ones_on_the_same_port() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let mut before = identified(&fake, None).await;

    let closed = fake.restart(Duration::from_millis(30)).await.unwrap();

    assert_eq!((closed, close_code(&mut before).await), (1, Some(1001)));
    let _after = identified(&fake, None).await;
    assert_eq!(fake.ledger().live_sessions().count(), 1);
}

#[tokio::test]
async fn an_offline_fake_refuses_connections_until_it_goes_online() {
    let fake = FakeObs::start(FakeObsConfig {
        online_at_boot: false,
        ..FakeObsConfig::default()
    })
    .await
    .unwrap();

    let refused = tokio_tungstenite::connect_async(fake.url()).await;
    fake.go_online().await.unwrap();
    let accepted = tokio_tungstenite::connect_async(fake.url()).await;

    assert!(
        refused.is_err() && accepted.is_ok(),
        "{:?}",
        refused.map(|_| ())
    );
}

#[tokio::test]
async fn lifecycle_changes_that_do_not_apply_to_the_current_state_are_refused() {
    let online = FakeObs::start(FakeObsConfig::default()).await.unwrap();
    let offline = FakeObs::start(FakeObsConfig {
        online_at_boot: false,
        ..FakeObsConfig::default()
    })
    .await
    .unwrap();

    let start_twice = online.go_online().await;
    let restart_offline = offline.restart(Duration::from_millis(1)).await;

    assert!(
        matches!(
            (&start_twice, &restart_offline),
            (
                Err(EmulatorError::FakeObsRefused { .. }),
                Err(EmulatorError::FakeObsRefused { .. })
            )
        ),
        "{start_twice:?} {restart_offline:?}"
    );
}

#[tokio::test]
async fn invalid_configurations_are_refused_before_listening() {
    let input = |name: &str| FakeInput {
        name: name.to_owned(),
        kind: "pulse_input_capture".to_owned(),
        muted: false,
    };
    for (label, config) in [
        (
            "no scenes",
            FakeObsConfig {
                scenes: Vec::new(),
                ..FakeObsConfig::default()
            },
        ),
        (
            "repeated scene",
            FakeObsConfig {
                scenes: vec!["Main".to_owned(), "Main".to_owned()],
                ..FakeObsConfig::default()
            },
        ),
        (
            "current scene not listed",
            FakeObsConfig {
                current_scene: Some("Elsewhere".to_owned()),
                ..FakeObsConfig::default()
            },
        ),
        (
            "repeated input",
            FakeObsConfig {
                inputs: vec![input("Mic"), input("Mic")],
                ..FakeObsConfig::default()
            },
        ),
        (
            "empty password",
            FakeObsConfig {
                password: Some(String::new()),
                ..FakeObsConfig::default()
            },
        ),
    ] {
        let outcome = FakeObs::start(config).await;
        assert!(
            matches!(outcome, Err(EmulatorError::InvalidFakeConfig { .. })),
            "{label}"
        );
    }
}

#[tokio::test]
async fn the_configured_current_scene_is_the_program_scene_at_start() {
    let fake = FakeObs::start(FakeObsConfig {
        current_scene: Some("BRB".to_owned()),
        ..with_password()
    })
    .await
    .unwrap();
    let mut socket = identified(&fake, None).await;

    let response = request(&mut socket, "GetCurrentProgramScene", json!({})).await;

    assert_eq!(response["responseData"]["sceneName"], "BRB");
}

fn pushed_types(fake: &FakeObs) -> Vec<String> {
    fake.ledger()
        .events
        .into_iter()
        .map(|event| event.event_type)
        .collect()
}

#[tokio::test]
async fn preview_requests_are_refused_as_studio_mode_not_active_while_studio_mode_is_off() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let mut socket = identified(&fake, None).await;

    let set = request(
        &mut socket,
        "SetCurrentPreviewScene",
        json!({ "sceneName": "BRB" }),
    )
    .await;
    let enabled = request(&mut socket, "GetStudioModeEnabled", json!({})).await;

    assert_eq!(
        (
            set["requestStatus"]["code"].clone(),
            enabled["responseData"]["studioModeEnabled"].clone(),
            pushed_types(&fake),
        ),
        (json!(506), json!(false), Vec::<String>::new())
    );
}

#[tokio::test]
async fn enabling_studio_mode_pushes_the_state_change_once_and_repeating_pushes_nothing() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let mut socket = identified(&fake, None).await;

    let first = request(
        &mut socket,
        "SetStudioModeEnabled",
        json!({ "studioModeEnabled": true }),
    )
    .await;
    request(
        &mut socket,
        "SetStudioModeEnabled",
        json!({ "studioModeEnabled": true }),
    )
    .await;
    let enabled = request(&mut socket, "GetStudioModeEnabled", json!({})).await;

    let events = fake.ledger().events;
    assert_eq!(
        (
            first["requestStatus"]["code"].clone(),
            enabled["responseData"]["studioModeEnabled"].clone(),
            events.len(),
            events[0].event_type.clone(),
            events[0].data.clone(),
        ),
        (
            json!(100),
            json!(true),
            1,
            "StudioModeStateChanged".to_owned(),
            json!({ "studioModeEnabled": true }),
        )
    );
}

#[tokio::test]
async fn setting_the_preview_scene_in_studio_mode_succeeds_and_pushes_the_preview_change() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let mut socket = identified(&fake, None).await;
    request(
        &mut socket,
        "SetStudioModeEnabled",
        json!({ "studioModeEnabled": true }),
    )
    .await;

    let set = request(
        &mut socket,
        "SetCurrentPreviewScene",
        json!({ "sceneName": "BRB" }),
    )
    .await;
    let preview = request(&mut socket, "GetCurrentPreviewScene", json!({})).await;
    let missing = request(&mut socket, "SetCurrentPreviewScene", json!({})).await;
    let unknown = request(
        &mut socket,
        "SetCurrentPreviewScene",
        json!({ "sceneName": "Nope" }),
    )
    .await;

    let events = fake.ledger().events;
    assert_eq!(
        (
            set["requestStatus"]["code"].clone(),
            preview["responseData"]["currentPreviewSceneName"].clone(),
            fake.current_scene(),
            missing["requestStatus"]["code"].clone(),
            unknown["requestStatus"]["code"].clone(),
            events.last().map(|event| event.event_type.clone()),
            events.last().map(|event| event.data["sceneName"].clone()),
        ),
        (
            json!(100),
            json!("BRB"),
            Some("Main".to_owned()),
            json!(300),
            json!(600),
            Some("CurrentPreviewSceneChanged".to_owned()),
            Some(json!("BRB")),
        )
    );
}

#[tokio::test]
async fn disabling_studio_mode_makes_preview_requests_refuse_again() {
    let fake = FakeObs::start(with_password()).await.unwrap();
    let mut socket = identified(&fake, None).await;
    for enabled in [true, false] {
        request(
            &mut socket,
            "SetStudioModeEnabled",
            json!({ "studioModeEnabled": enabled }),
        )
        .await;
    }

    let set = request(
        &mut socket,
        "SetCurrentPreviewScene",
        json!({ "sceneName": "BRB" }),
    )
    .await;
    let missing_flag = request(&mut socket, "SetStudioModeEnabled", json!({})).await;

    assert_eq!(
        (
            set["requestStatus"]["code"].clone(),
            missing_flag["requestStatus"]["code"].clone(),
        ),
        (json!(506), json!(300))
    );
}
