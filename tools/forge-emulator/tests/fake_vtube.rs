#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_emulator::EmulatorError;
use forge_emulator::vtube::{
    FakeModel, FakeVTube, FakeVTubeConfig, HOTKEY_TRIGGERED_EVENT, ITEM_EVENT, MODEL_LOADED_EVENT,
    TokenCheck, VTubeLedger, hotkey_id, model_id,
};
use forge_events::{Event, EventPublisher};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::Variant;
use forge_vtube::{VTUBE_CREDENTIAL_ID, VTubeClient, VTubeSink, probe_connection};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const TOKEN: &str = "vtube-fake-token-3f9a";
const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);
const LOCALHOST: &str = "127.0.0.1";
const AVATAR: &str = "Avatar";
const SPARE: &str = "Spare";
const WAVE: &str = "Wave";
const BLUSH: &str = "Blush.exp3.json";
const STAR: &str = "star.png";

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

type StudioChange = Box<dyn Fn(&FakeVTube) -> Result<usize, EmulatorError>>;

fn config() -> FakeVTubeConfig {
    FakeVTubeConfig {
        token: TOKEN.to_owned(),
        models: vec![
            FakeModel {
                name: AVATAR.to_owned(),
                hotkeys: vec![WAVE.to_owned(), "Spin".to_owned()],
                expressions: vec![BLUSH.to_owned()],
            },
            FakeModel {
                name: SPARE.to_owned(),
                hotkeys: Vec::new(),
                expressions: Vec::new(),
            },
        ],
        items: vec![STAR.to_owned()],
        ..FakeVTubeConfig::default()
    }
}

async fn start() -> FakeVTube {
    FakeVTube::start(config()).await.unwrap()
}

async fn open(fake: &FakeVTube) -> Socket {
    tokio_tungstenite::connect_async(fake.url())
        .await
        .unwrap()
        .0
}

async fn next_frame(socket: &mut Socket) -> Option<Value> {
    loop {
        match timeout(WAIT, socket.next()).await.ok()?? {
            Ok(Message::Text(text)) => return serde_json::from_str(text.as_str()).ok(),
            Ok(Message::Close(_)) | Err(_) => return None,
            Ok(_) => {}
        }
    }
}

async fn send_text(socket: &mut Socket, text: String) {
    socket.send(Message::Text(text.into())).await.unwrap();
}

fn envelope(message_type: &str, request_id: &str, data: Value) -> Value {
    json!({
        "apiName": "VTubeStudioPublicAPI",
        "apiVersion": "1.0",
        "requestID": request_id,
        "messageType": message_type,
        "data": data,
    })
}

async fn reply_to(socket: &mut Socket, request_id: &str) -> Value {
    loop {
        let frame = next_frame(socket).await.expect("a reply frame");
        if frame["requestID"] == request_id {
            return frame;
        }
    }
}

async fn call(socket: &mut Socket, message_type: &str, data: Value) -> Value {
    let request_id = format!("req-{}", NEXT_REQUEST.fetch_add(1, Ordering::Relaxed));
    send_text(
        socket,
        envelope(message_type, &request_id, data).to_string(),
    )
    .await;
    reply_to(socket, &request_id).await
}

fn error_id(reply: &Value) -> Option<i64> {
    (reply["messageType"] == "APIError").then(|| reply["data"]["errorID"].as_i64().unwrap())
}

async fn authenticate(socket: &mut Socket, token: &str) -> Value {
    call(
        socket,
        "AuthenticationRequest",
        json!({ "pluginName": "forge", "pluginDeveloper": "forge", "authenticationToken": token }),
    )
    .await
}

async fn authed(fake: &FakeVTube) -> Socket {
    let mut socket = open(fake).await;
    let reply = authenticate(&mut socket, TOKEN).await;
    assert_eq!(reply["data"]["authenticated"], true, "{reply}");
    socket
}

async fn subscribe(socket: &mut Socket, event_name: &str) {
    let reply = call(
        socket,
        "EventSubscriptionRequest",
        json!({ "eventName": event_name, "subscribe": true }),
    )
    .await;
    assert_eq!(error_id(&reply), None, "{reply}");
}

async fn next_event(socket: &mut Socket, event_name: &str) -> Value {
    loop {
        let frame = next_frame(socket)
            .await
            .unwrap_or_else(|| panic!("no {event_name} arrived"));
        if frame["messageType"] == event_name {
            return frame["data"].clone();
        }
    }
}

async fn wait_ledger<T>(
    fake: &FakeVTube,
    probe: impl FnMut(&VTubeLedger) -> Option<T>,
) -> Result<T, EmulatorError> {
    fake.wait_for("ledger state", WAIT, probe).await
}

#[derive(Default)]
struct Captured(Mutex<Vec<Event>>);

impl EventPublisher for Captured {
    fn publish(&self, event: Event) {
        self.0.lock().unwrap().push(event);
    }
}

impl Captured {
    fn find(&self, matches: impl Fn(&Event) -> bool) -> Option<Event> {
        self.0.lock().unwrap().iter().find(|e| matches(e)).cloned()
    }

    async fn wait_for(&self, matches: impl Fn(&Event) -> bool) -> Option<Event> {
        let deadline = tokio::time::Instant::now() + WAIT;
        while tokio::time::Instant::now() < deadline {
            if let Some(event) = self.find(&matches) {
                return Some(event);
            }
            tokio::time::sleep(POLL).await;
        }
        None
    }
}

#[derive(Default)]
struct MemoryCredentials(Mutex<HashMap<String, String>>);

#[async_trait]
impl CredentialsRepo for MemoryCredentials {
    async fn store(&self, id: &CredentialId, plaintext: &str) -> Result<(), StorageError> {
        self.0
            .lock()
            .unwrap()
            .insert(id.as_str().to_owned(), plaintext.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .keys()
            .map(|key| CredentialId::new(key.clone()))
            .collect())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

struct Forge {
    client: Arc<VTubeClient>,
    events: Arc<Captured>,
    credentials: Arc<MemoryCredentials>,
}

async fn forge_with_token(fake: &FakeVTube, token: &str) -> Forge {
    let credentials = Arc::new(MemoryCredentials::default());
    forge_vtube::credentials::store(&*credentials, token, "1.0", LOCALHOST, fake.port())
        .await
        .unwrap();
    let events = Arc::new(Captured::default());
    let client = forge_vtube::credentials::load_and_connect(
        &*credentials,
        Arc::clone(&events) as Arc<dyn EventPublisher>,
        Arc::clone(&credentials) as Arc<dyn CredentialsRepo>,
    )
    .await
    .unwrap();
    Forge {
        client,
        events,
        credentials,
    }
}

fn is_connected(event: &Event) -> bool {
    event.kind == "vtube.connection.changed" && event.payload["is_connected"] == true
}

async fn connected_forge(fake: &FakeVTube) -> Forge {
    let forge = forge_with_token(fake, TOKEN).await;
    forge
        .events
        .wait_for(is_connected)
        .await
        .expect("forge connects to the fake VTube Studio");
    wait_ledger(fake, |ledger| {
        ledger
            .requests
            .iter()
            .any(|request| request.message_type == "FaceFoundRequest")
            .then_some(())
    })
    .await
    .unwrap();
    forge
}

#[tokio::test]
async fn forge_with_a_stored_token_authenticates_and_subscribes_to_every_event_it_maps() {
    let fake = start().await;
    let forge = connected_forge(&fake).await;

    let session = wait_ledger(&fake, |ledger| ledger.live_sessions().next().cloned())
        .await
        .unwrap();

    assert_eq!(session.authentication, TokenCheck::Accepted);
    assert!(!session.token_requested);
    for event in [
        "ModelLoadedEvent",
        "ModelConfigChangedEvent",
        "HotkeyTriggeredEvent",
        "TrackingStatusChangedEvent",
        "ItemEvent",
    ] {
        assert!(
            session.subscriptions.iter().any(|known| known == event),
            "{event} missing from {:?}",
            session.subscriptions
        );
    }
    forge.client.shutdown().await;
}

#[tokio::test]
async fn forge_with_a_revoked_token_reports_auth_required_and_forgets_the_token() {
    let fake = start().await;
    let forge = forge_with_token(&fake, "a-revoked-token").await;

    let reported = forge
        .events
        .wait_for(|event| {
            event.kind == "vtube.connection.changed" && event.payload["reason"] == "auth_required"
        })
        .await;

    assert!(reported.is_some(), "forge never reported auth_required");
    assert_eq!(
        fake.ledger().sessions[0].authentication,
        TokenCheck::Rejected
    );
    assert!(
        forge
            .credentials
            .load(&CredentialId::new(VTUBE_CREDENTIAL_ID))
            .await
            .unwrap()
            .is_none()
    );
    forge.client.shutdown().await;
}

#[tokio::test]
async fn forge_probe_reads_the_version_of_a_session_that_is_not_authenticated() {
    let fake = start().await;

    let probe = probe_connection(LOCALHOST, fake.port()).await.unwrap();

    assert!(!probe.api_version.is_empty());
    assert!(!probe.already_authenticated);
}

#[tokio::test]
async fn every_request_forge_sub_actions_send_is_answered_without_an_error() {
    let fake = start().await;
    let forge = connected_forge(&fake).await;
    let sink: &dyn VTubeSink = &*forge.client;

    let item = sink
        .load_item(STAR, Some(0.1), Some(0.2), None, None, None, None, true)
        .await
        .unwrap();
    let Variant::Object(item) = item else {
        panic!("load_item returned {item:?}");
    };
    let Some(Variant::String(instance)) = item.get("instance_id").cloned() else {
        panic!("load_item returned no instance id: {item:?}");
    };
    let outcomes = [
        ("hotkey", sink.trigger_hotkey(WAVE).await.map(drop)),
        (
            "expression",
            sink.set_expression(BLUSH, true).await.map(drop),
        ),
        ("param", sink.set_param("MouthOpen", 0.5).await.map(drop)),
        ("reset", sink.reset_params().await.map(drop)),
        (
            "move model",
            sink.move_model(Some(0.1), None, None, Some(-20.0), 0.5)
                .await
                .map(drop),
        ),
        (
            "move item",
            sink.move_item(&instance, Some(0.3), None, None, None, None, 0.5, "linear")
                .await
                .map(drop),
        ),
        ("current model", sink.get_current_model().await.map(drop)),
        ("hotkeys", sink.get_hotkeys().await.map(drop)),
        ("expressions", sink.get_expressions().await.map(drop)),
        ("parameters", sink.get_parameters().await.map(drop)),
        ("items", sink.get_items().await.map(drop)),
        (
            "pin",
            sink.pin_item(
                &instance,
                true,
                "RelativeToModel",
                "RelativeToWorld",
                "Center",
                "",
                "",
                0.0,
                0.3,
            )
            .await
            .map(drop),
        ),
        (
            "tint",
            sink.tint_all_art_meshes(255, 128, 0, 255, Some(1.0))
                .await
                .map(drop),
        ),
        (
            "physics",
            sink.set_physics_override(1.5, 2.0).await.map(drop),
        ),
        ("unload items", sink.unload_all_items().await.map(drop)),
        ("load model", sink.load_model(&model_id(1)).await.map(drop)),
    ];

    let failed: Vec<String> = outcomes
        .iter()
        .filter_map(|(name, outcome)| outcome.as_ref().err().map(|e| format!("{name}: {e}")))
        .collect();
    assert!(failed.is_empty(), "{failed:?}");
    let refused: Vec<String> = fake
        .ledger()
        .requests
        .iter()
        .filter(|request| request.error_id.is_some())
        .map(|request| format!("{} {:?}", request.message_type, request.error_id))
        .collect();
    assert!(refused.is_empty(), "{refused:?}");
    forge.client.shutdown().await;
}

#[tokio::test]
async fn forge_lookups_read_back_the_state_the_fake_holds() {
    let fake = start().await;
    let forge = connected_forge(&fake).await;
    let sink: &dyn VTubeSink = &*forge.client;
    sink.load_item(STAR, None, None, None, None, None, None, false)
        .await
        .unwrap();

    let hotkeys = sink.get_hotkeys().await.unwrap();
    let items = sink.get_items().await.unwrap();
    let model = sink.get_current_model().await.unwrap();

    let field = |value: &Variant, key: &str| match value {
        Variant::Object(fields) => fields.get(key).cloned(),
        _ => None,
    };
    assert_eq!(
        field(&hotkeys, "ids"),
        Some(Variant::Array(vec![
            Variant::String(hotkey_id(0, 0)),
            Variant::String(hotkey_id(0, 1)),
        ]))
    );
    assert_eq!(
        field(&items, "file_names"),
        Some(Variant::Array(vec![Variant::String(STAR.to_owned())]))
    );
    assert_eq!(
        field(&model, "name"),
        Some(Variant::String(AVATAR.to_owned()))
    );
    forge.client.shutdown().await;
}

#[tokio::test]
async fn a_hotkey_forge_triggers_comes_back_as_a_triggered_event() {
    let fake = start().await;
    let forge = connected_forge(&fake).await;

    forge.client.trigger_hotkey(WAVE).await.unwrap();

    let event = forge
        .events
        .wait_for(|event| event.kind == "vtube.hotkey.triggered")
        .await
        .expect("vtube.hotkey.triggered");
    assert_eq!(event.payload["hotkey_name"], WAVE);
    assert_eq!(event.payload["hotkey_id"], hotkey_id(0, 0));
    forge.client.shutdown().await;
}

#[tokio::test]
async fn studio_changes_reach_forge_as_its_vtube_events() {
    let fake = start().await;
    let forge = connected_forge(&fake).await;
    let cases: [(&str, StudioChange); 7] = [
        (
            "vtube.hotkey.triggered",
            Box::new(|f| f.trigger_hotkey(WAVE)),
        ),
        (
            "vtube.tracking.face_lost",
            Box::new(|f| f.set_face_found(false)),
        ),
        (
            "vtube.tracking.face_found",
            Box::new(|f| f.set_face_found(true)),
        ),
        ("vtube.item.added", Box::new(|f| f.add_item(STAR))),
        ("vtube.item.removed", Box::new(|f| f.remove_item(STAR))),
        (
            "vtube.model.config_changed",
            Box::new(FakeVTube::change_model_config),
        ),
        ("vtube.model.unloaded", Box::new(FakeVTube::unload_model)),
    ];

    for (kind, change) in cases {
        assert_eq!(change(&fake).unwrap(), 1, "{kind} was not delivered");
        assert!(
            forge
                .events
                .wait_for(|event| event.kind == kind)
                .await
                .is_some(),
            "forge did not publish {kind}"
        );
    }
    fake.load_model(AVATAR).unwrap();
    assert!(
        forge
            .events
            .wait_for(
                |event| event.kind == "vtube.model.loaded" && event.payload["model_name"] == AVATAR
            )
            .await
            .is_some()
    );
    forge.client.shutdown().await;
}

#[tokio::test]
async fn an_expression_turned_on_in_the_studio_reaches_forge_through_its_state_poll() {
    let fake = start().await;
    let forge = connected_forge(&fake).await;
    wait_ledger(&fake, |ledger| {
        ledger
            .requests
            .iter()
            .any(|request| {
                request.message_type == "ExpressionStateRequest" && request.data["details"] == false
            })
            .then_some(())
    })
    .await
    .unwrap();

    fake.set_expression(BLUSH, true).unwrap();

    let event = forge
        .events
        .wait_for(|event| event.kind == "vtube.expression.state_changed")
        .await
        .expect("vtube.expression.state_changed");
    assert_eq!(event.payload["expression_file"], BLUSH);
    assert_eq!(event.payload["is_active"], true);
    forge.client.shutdown().await;
}

#[tokio::test]
async fn api_state_reports_the_session_authenticated_only_after_a_valid_token() {
    let fake = start().await;
    let mut socket = open(&fake).await;

    let before = call(&mut socket, "APIStateRequest", json!({})).await;
    authenticate(&mut socket, TOKEN).await;
    let after = call(&mut socket, "APIStateRequest", json!({})).await;

    assert_eq!(before["data"]["currentSessionAuthenticated"], false);
    assert_eq!(after["data"]["currentSessionAuthenticated"], true);
    assert_eq!(after["messageType"], "APIStateResponse");
}

#[tokio::test]
async fn plugin_requests_are_refused_until_the_session_authenticates() {
    let fake = start().await;
    let mut socket = open(&fake).await;

    let before = call(&mut socket, "CurrentModelRequest", json!({})).await;
    authenticate(&mut socket, TOKEN).await;
    let after = call(&mut socket, "CurrentModelRequest", json!({})).await;

    assert_eq!(error_id(&before), Some(8));
    assert_eq!(error_id(&after), None);
    assert_eq!(after["data"]["modelName"], AVATAR);
}

#[tokio::test]
async fn a_wrong_token_is_answered_unauthenticated_and_recorded_as_rejected() {
    let fake = start().await;
    let mut socket = open(&fake).await;

    let reply = authenticate(&mut socket, "not-the-token").await;
    let refused = call(&mut socket, "HotkeysInCurrentModelRequest", json!({})).await;

    assert_eq!(reply["messageType"], "AuthenticationResponse");
    assert_eq!(reply["data"]["authenticated"], false);
    assert_eq!(error_id(&refused), Some(8));
    assert_eq!(
        fake.ledger().sessions[0].authentication,
        TokenCheck::Rejected
    );
}

#[tokio::test]
async fn authentication_missing_a_field_is_refused_with_that_field_error() {
    let fake = start().await;
    let mut socket = open(&fake).await;
    let complete =
        json!({ "pluginName": "forge", "pluginDeveloper": "forge", "authenticationToken": TOKEN });
    for (missing, expected) in [
        ("authenticationToken", 100),
        ("pluginName", 101),
        ("pluginDeveloper", 102),
    ] {
        let mut data = complete.clone();
        data.as_object_mut().unwrap().remove(missing);

        let reply = call(&mut socket, "AuthenticationRequest", data).await;

        assert_eq!(error_id(&reply), Some(expected), "without {missing}");
    }
}

#[tokio::test]
async fn token_requests_are_answered_with_the_token_or_denied_as_configured() {
    for (approve, expected_error) in [(true, None), (false, Some(50))] {
        let fake = FakeVTube::start(FakeVTubeConfig {
            approve_token_requests: approve,
            ..config()
        })
        .await
        .unwrap();
        let mut socket = open(&fake).await;

        let reply = call(
            &mut socket,
            "AuthenticationTokenRequest",
            json!({ "pluginName": "forge", "pluginDeveloper": "forge" }),
        )
        .await;

        assert_eq!(error_id(&reply), expected_error, "approve {approve}");
        if approve {
            assert_eq!(reply["data"]["authenticationToken"], TOKEN);
        }
        assert!(fake.ledger().sessions[0].token_requested);
    }
}

#[tokio::test]
async fn token_requests_need_plugin_and_developer_names_of_three_to_thirty_two_characters() {
    let fake = start().await;
    let mut socket = open(&fake).await;
    let longest = "x".repeat(32);
    let too_long = "x".repeat(33);
    for (name, developer, expected) in [
        ("abc", "abc", None),
        (longest.as_str(), longest.as_str(), None),
        ("ab", "abc", Some(52)),
        (too_long.as_str(), "abc", Some(52)),
        ("abc", "ab", Some(53)),
        ("abc", too_long.as_str(), Some(53)),
    ] {
        let reply = call(
            &mut socket,
            "AuthenticationTokenRequest",
            json!({ "pluginName": name, "pluginDeveloper": developer }),
        )
        .await;

        assert_eq!(error_id(&reply), expected, "{name} / {developer}");
    }
}

#[tokio::test]
async fn malformed_envelopes_are_answered_with_the_spec_error_ids() {
    let fake = start().await;
    let mut socket = open(&fake).await;
    let request = |field: &str, value: Value| {
        let mut frame = envelope("APIStateRequest", "env", json!({}));
        frame[field] = value;
        frame.to_string()
    };
    let without_type = {
        let mut frame = envelope("APIStateRequest", "env", json!({}));
        frame.as_object_mut().unwrap().remove("messageType");
        frame.to_string()
    };
    for (what, text, expected) in [
        ("not json", "{ not json".to_owned(), 2),
        ("api name", request("apiName", json!("SomeOtherAPI")), 3),
        ("api version", request("apiVersion", json!("2.0")), 4),
        (
            "request id of 65 chars",
            request("requestID", json!("r".repeat(65))),
            5,
        ),
        ("empty request id", request("requestID", json!("")), 5),
        ("no message type", without_type, 6),
    ] {
        send_text(&mut socket, text).await;
        let reply = next_frame(&mut socket).await.unwrap();

        assert_eq!(error_id(&reply), Some(expected), "{what}: {reply}");
    }
}

#[tokio::test]
async fn a_request_id_of_sixty_four_characters_is_echoed_back() {
    let fake = start().await;
    let mut socket = open(&fake).await;
    let request_id = "r".repeat(64);

    send_text(
        &mut socket,
        envelope("APIStateRequest", &request_id, json!({})).to_string(),
    )
    .await;
    let reply = next_frame(&mut socket).await.unwrap();

    assert_eq!(reply["requestID"], request_id.as_str());
    assert_eq!(error_id(&reply), None);
}

#[tokio::test]
async fn an_unknown_request_type_is_refused_once_authenticated() {
    let fake = start().await;
    let mut socket = authed(&fake).await;

    let reply = call(&mut socket, "NoSuchRequest", json!({})).await;

    assert_eq!(error_id(&reply), Some(7));
}

#[tokio::test]
async fn events_reach_only_authenticated_sessions_subscribed_to_them() {
    let fake = start().await;
    let mut subscriber = authed(&fake).await;
    subscribe(&mut subscriber, HOTKEY_TRIGGERED_EVENT).await;
    let _bystander = authed(&fake).await;
    let mut anonymous = open(&fake).await;
    let refused = call(
        &mut anonymous,
        "EventSubscriptionRequest",
        json!({ "eventName": HOTKEY_TRIGGERED_EVENT, "subscribe": true }),
    )
    .await;

    let delivered = fake.trigger_hotkey(WAVE).unwrap();
    let event = next_event(&mut subscriber, HOTKEY_TRIGGERED_EVENT).await;

    assert_eq!(error_id(&refused), Some(8));
    assert_eq!(delivered, 1);
    assert_eq!(event["hotkeyName"], WAVE);
    assert_eq!(event["hotkeyTriggeredByAPI"], false);
    assert_eq!(event["modelID"], model_id(0));
}

#[tokio::test]
async fn unsubscribing_without_an_event_name_drops_every_subscription() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    subscribe(&mut socket, HOTKEY_TRIGGERED_EVENT).await;
    subscribe(&mut socket, ITEM_EVENT).await;

    let reply = call(
        &mut socket,
        "EventSubscriptionRequest",
        json!({ "subscribe": false }),
    )
    .await;

    assert_eq!(reply["data"]["subscribedEventCount"], 0);
    assert_eq!(fake.trigger_hotkey(WAVE).unwrap(), 0);
}

#[tokio::test]
async fn subscribing_to_an_unknown_event_is_refused() {
    let fake = start().await;
    let mut socket = authed(&fake).await;

    let reply = call(
        &mut socket,
        "EventSubscriptionRequest",
        json!({ "eventName": "NoSuchEvent", "subscribe": true }),
    )
    .await;

    assert_eq!(error_id(&reply), Some(950));
}

#[tokio::test]
async fn hotkey_triggers_match_by_id_or_case_insensitive_name_and_echo_a_triggered_by_api_event() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    subscribe(&mut socket, HOTKEY_TRIGGERED_EVENT).await;
    for requested in [hotkey_id(0, 0), "wave".to_owned()] {
        let reply = call(
            &mut socket,
            "HotkeyTriggerRequest",
            json!({ "hotkeyID": requested }),
        )
        .await;
        let event = next_event(&mut socket, HOTKEY_TRIGGERED_EVENT).await;

        assert_eq!(reply["data"]["hotkeyID"], hotkey_id(0, 0), "{requested}");
        assert_eq!(event["hotkeyTriggeredByAPI"], true, "{requested}");
    }
}

#[tokio::test]
async fn hotkey_triggers_that_cannot_run_are_refused_with_their_error_ids() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let unknown = call(
        &mut socket,
        "HotkeyTriggerRequest",
        json!({ "hotkeyID": "NoSuchHotkey" }),
    )
    .await;
    let in_item = call(
        &mut socket,
        "HotkeyTriggerRequest",
        json!({ "hotkeyID": WAVE, "itemInstanceID": "missing" }),
    )
    .await;
    fake.unload_model().unwrap();
    let no_model = call(
        &mut socket,
        "HotkeyTriggerRequest",
        json!({ "hotkeyID": WAVE }),
    )
    .await;

    assert_eq!(error_id(&unknown), Some(202));
    assert_eq!(error_id(&in_item), Some(207));
    assert_eq!(error_id(&no_model), Some(201));
}

#[tokio::test]
async fn loading_another_model_pushes_the_old_one_unloaded_then_the_new_one_loaded() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    subscribe(&mut socket, MODEL_LOADED_EVENT).await;

    let reply = call(
        &mut socket,
        "ModelLoadRequest",
        json!({ "modelID": model_id(1) }),
    )
    .await;
    let first = next_event(&mut socket, MODEL_LOADED_EVENT).await;
    let second = next_event(&mut socket, MODEL_LOADED_EVENT).await;

    assert_eq!(reply["data"]["modelID"], model_id(1));
    assert_eq!(
        (&first["modelName"], &first["modelLoaded"]),
        (&json!(AVATAR), &json!(false))
    );
    assert_eq!(
        (&second["modelName"], &second["modelLoaded"]),
        (&json!(SPARE), &json!(true))
    );
    assert_eq!(fake.current_model().as_deref(), Some(SPARE));
}

#[tokio::test]
async fn an_empty_model_id_unloads_the_model_and_an_unknown_one_is_refused() {
    let fake = start().await;
    let mut socket = authed(&fake).await;

    let unknown = call(
        &mut socket,
        "ModelLoadRequest",
        json!({ "modelID": "ffffffffffffffffffffffffffffffff" }),
    )
    .await;
    let missing = call(&mut socket, "ModelLoadRequest", json!({})).await;
    let unload = call(&mut socket, "ModelLoadRequest", json!({ "modelID": "" })).await;

    assert_eq!(error_id(&unknown), Some(152));
    assert_eq!(error_id(&missing), Some(150));
    assert_eq!(error_id(&unload), None);
    assert_eq!(fake.current_model(), None);
}

#[tokio::test]
async fn expression_activation_flips_the_state_the_expression_list_reports() {
    let fake = start().await;
    let mut socket = authed(&fake).await;

    let reply = call(
        &mut socket,
        "ExpressionActivationRequest",
        json!({ "expressionFile": BLUSH, "active": true }),
    )
    .await;
    let state = call(
        &mut socket,
        "ExpressionStateRequest",
        json!({ "details": false, "expressionFile": BLUSH }),
    )
    .await;

    assert_eq!(error_id(&reply), None);
    assert_eq!(state["data"]["expressions"][0]["active"], true);
    assert_eq!(state["data"]["expressions"][0]["name"], "Blush");
    assert_eq!(fake.expression_active(BLUSH), Some(true));
}

#[tokio::test]
async fn expression_requests_naming_bad_files_are_refused_with_their_error_ids() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let cases = [
        ("ExpressionActivationRequest", "Blush.json", 650),
        ("ExpressionActivationRequest", "Missing.exp3.json", 651),
        ("ExpressionStateRequest", "Blush.json", 600),
        ("ExpressionStateRequest", "Missing.exp3.json", 601),
    ];
    for (request, file, expected) in cases {
        let reply = call(
            &mut socket,
            request,
            json!({ "expressionFile": file, "active": true }),
        )
        .await;

        assert_eq!(error_id(&reply), Some(expected), "{request} {file}");
    }
    fake.unload_model().unwrap();
    let no_model = call(
        &mut socket,
        "ExpressionActivationRequest",
        json!({ "expressionFile": BLUSH, "active": true }),
    )
    .await;
    assert_eq!(error_id(&no_model), Some(652));
}

#[tokio::test]
async fn loading_an_item_places_it_in_the_scene_and_pushes_it_added() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    subscribe(&mut socket, ITEM_EVENT).await;

    let reply = call(
        &mut socket,
        "ItemLoadRequest",
        json!({ "fileName": STAR, "positionX": 0.25, "positionY": -0.5, "order": 4 }),
    )
    .await;
    let event = next_event(&mut socket, ITEM_EVENT).await;
    let list = call(
        &mut socket,
        "ItemListRequest",
        json!({ "includeItemInstancesInScene": true }),
    )
    .await;

    let instance = reply["data"]["instanceID"].clone();
    assert_eq!(event["itemEventType"], "Added");
    assert_eq!(event["itemInstanceID"], instance);
    assert_eq!(event["itemPosition"], json!({ "x": 0.25, "y": -0.5 }));
    assert_eq!(list["data"]["itemsInSceneCount"], 1);
    assert_eq!(list["data"]["itemInstancesInScene"][0]["order"], 4);
}

#[tokio::test]
async fn item_loads_that_cannot_run_are_refused_with_their_error_ids() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    call(
        &mut socket,
        "ItemLoadRequest",
        json!({ "fileName": STAR, "order": 4 }),
    )
    .await;
    let cases = [
        ("missing file name", json!({}), 750),
        ("unknown file", json!({ "fileName": "nope.png" }), 751),
        (
            "custom data without permission",
            json!({ "fileName": STAR, "customDataBase64": "iVBORw0K" }),
            9,
        ),
        (
            "order taken",
            json!({ "fileName": STAR, "order": 4, "failIfOrderTaken": true }),
            756,
        ),
        (
            "size above one",
            json!({ "fileName": STAR, "size": 1.01 }),
            757,
        ),
        (
            "position past the edge",
            json!({ "fileName": STAR, "positionX": -1000.5 }),
            757,
        ),
        (
            "fade above two seconds",
            json!({ "fileName": STAR, "fadeTime": 2.01 }),
            757,
        ),
    ];
    for (what, data, expected) in cases {
        let reply = call(&mut socket, "ItemLoadRequest", data).await;

        assert_eq!(error_id(&reply), Some(expected), "{what}");
    }
}

#[tokio::test]
async fn an_item_loaded_at_a_taken_order_lands_on_the_next_free_order() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let load =
        |order: i64| json!({ "fileName": STAR, "order": order, "size": 1.0, "positionX": 1000.0 });

    call(&mut socket, "ItemLoadRequest", load(4)).await;
    let second = call(&mut socket, "ItemLoadRequest", load(4)).await;
    let list = call(
        &mut socket,
        "ItemListRequest",
        json!({ "includeItemInstancesInScene": true, "onlyItemsWithInstanceID": second["data"]["instanceID"] }),
    )
    .await;

    assert_eq!(error_id(&second), None);
    assert_eq!(list["data"]["itemInstancesInScene"][0]["order"], 5);
}

#[tokio::test]
async fn unloading_every_item_empties_the_scene_and_pushes_each_removed() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    fake.add_item(STAR).unwrap();
    call(&mut socket, "ItemLoadRequest", json!({ "fileName": STAR })).await;
    subscribe(&mut socket, ITEM_EVENT).await;

    let reply = call(
        &mut socket,
        "ItemUnloadRequest",
        json!({ "unloadAllInScene": true }),
    )
    .await;
    let first = next_event(&mut socket, ITEM_EVENT).await;
    let second = next_event(&mut socket, ITEM_EVENT).await;

    assert_eq!(reply["data"]["unloadedItems"].as_array().unwrap().len(), 2);
    assert_eq!(first["itemEventType"], "Removed");
    assert_eq!(second["itemEventType"], "Removed");
    assert!(fake.items_in_scene().is_empty());
}

#[tokio::test]
async fn unloading_only_this_plugins_items_keeps_the_ones_the_streamer_added() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    fake.add_item(STAR).unwrap();
    call(&mut socket, "ItemLoadRequest", json!({ "fileName": STAR })).await;

    let reply = call(
        &mut socket,
        "ItemUnloadRequest",
        json!({ "unloadAllLoadedByThisPlugin": true }),
    )
    .await;

    assert_eq!(reply["data"]["unloadedItems"].as_array().unwrap().len(), 1);
    assert_eq!(fake.items_in_scene(), [STAR]);
}

#[tokio::test]
async fn item_moves_report_success_or_the_error_of_each_item() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let loaded = call(&mut socket, "ItemLoadRequest", json!({ "fileName": STAR })).await;
    let instance = loaded["data"]["instanceID"].as_str().unwrap().to_owned();

    let reply = call(
        &mut socket,
        "ItemMoveRequest",
        json!({ "itemsToMove": [
            { "itemInstanceID": instance, "fadeMode": "zip", "order": -1000 },
            { "itemInstanceID": "missing" },
            { "itemInstanceID": instance, "fadeMode": "bounce" },
            { "itemInstanceID": instance, "order": 0 },
        ] }),
    )
    .await;

    let outcomes: Vec<(bool, i64)> = reply["data"]["movedItems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|moved| {
            (
                moved["success"].as_bool().unwrap(),
                moved["errorID"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        outcomes,
        [(true, -1), (false, 900), (false, 901), (false, 902)]
    );
}

#[tokio::test]
async fn item_pins_that_cannot_run_are_refused_with_their_error_ids() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let loaded = call(&mut socket, "ItemLoadRequest", json!({ "fileName": STAR })).await;
    let instance = loaded["data"]["instanceID"].clone();
    let pin = |info: Value, vertex: &str| {
        json!({
            "pin": true,
            "itemInstanceID": instance,
            "angleRelativeTo": "RelativeToModel",
            "sizeRelativeTo": "RelativeToWorld",
            "vertexPinType": vertex,
            "pinInfo": info,
        })
    };
    let cases = [
        ("pinned to the center", pin(json!({}), "Center"), None),
        (
            "unknown vertex pin type",
            pin(json!({}), "Corner"),
            Some(1051),
        ),
        (
            "other model",
            pin(json!({ "modelID": model_id(1) }), "Center"),
            Some(1052),
        ),
        (
            "unknown art mesh",
            pin(json!({ "artMeshID": "hair" }), "Center"),
            Some(1053),
        ),
        (
            "item not loaded",
            json!({ "pin": false, "itemInstanceID": "missing" }),
            Some(1050),
        ),
    ];
    for (what, data, expected) in cases {
        let reply = call(&mut socket, "ItemPinRequest", data).await;

        assert_eq!(error_id(&reply), expected, "{what}");
    }
}

#[tokio::test]
async fn parameter_injection_checks_ids_modes_values_and_weights() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let inject = |value: Value| json!({ "parameterValues": [value] });
    let cases = [
        (
            "value at the limit",
            inject(json!({ "id": "MouthOpen", "value": 1_000_000.0 })),
            None,
        ),
        (
            "value past the limit",
            inject(json!({ "id": "MouthOpen", "value": 1_000_000.5 })),
            Some(451),
        ),
        (
            "weight above one",
            inject(json!({ "id": "MouthOpen", "value": 0.0, "weight": 1.01 })),
            Some(452),
        ),
        (
            "unknown parameter",
            inject(json!({ "id": "TailWag", "value": 0.0 })),
            Some(453),
        ),
        ("no values", json!({ "parameterValues": [] }), Some(450)),
        (
            "unknown mode",
            json!({ "mode": "replace", "parameterValues": [{ "id": "MouthOpen", "value": 0.0 }] }),
            Some(455),
        ),
    ];
    for (what, data, expected) in cases {
        let reply = call(&mut socket, "InjectParameterDataRequest", data).await;

        assert_eq!(error_id(&reply), expected, "{what}");
    }
}

#[tokio::test]
async fn color_tint_needs_every_channel_within_zero_to_two_hundred_fifty_five() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let tint = |alpha: Value| {
        json!({
            "colorTint": { "colorR": 255, "colorG": 0, "colorB": 0, "colorA": alpha },
            "artMeshMatcher": { "tintAll": true },
        })
    };
    let cases = [
        ("channel at 255", tint(json!(255)), None),
        ("channel at 256", tint(json!(256)), Some(252)),
        ("channel below zero", tint(json!(-1)), Some(252)),
        ("channel missing", tint(Value::Null), Some(251)),
        (
            "matcher missing",
            json!({ "colorTint": { "colorR": 1, "colorG": 1, "colorB": 1, "colorA": 1 } }),
            Some(251),
        ),
    ];
    for (what, data, expected) in cases {
        let reply = call(&mut socket, "ColorTintRequest", data).await;

        assert_eq!(error_id(&reply), expected, "{what}");
    }
}

#[tokio::test]
async fn model_moves_need_their_required_fields_and_at_most_two_seconds() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let cases = [
        (
            "two seconds",
            json!({ "timeInSeconds": 2.0, "valuesAreRelativeToModel": false }),
            None,
        ),
        (
            "over two seconds",
            json!({ "timeInSeconds": 2.1, "valuesAreRelativeToModel": false }),
            Some(302),
        ),
        (
            "no relative flag",
            json!({ "timeInSeconds": 0.5 }),
            Some(301),
        ),
    ];
    for (what, data, expected) in cases {
        let reply = call(&mut socket, "MoveModelRequest", data).await;

        assert_eq!(error_id(&reply), expected, "{what}");
    }
}

#[tokio::test]
async fn physics_overrides_need_a_value_and_only_base_values_have_no_group() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    let strength = |entry: Value| json!({ "strengthOverrides": [entry], "windOverrides": [] });
    let cases = [
        (
            "base value",
            strength(json!({ "id": "", "value": 1.5, "setBaseValue": true, "overrideSeconds": 2 })),
            None,
        ),
        (
            "unknown group",
            strength(
                json!({ "id": "Hair", "value": 1.5, "setBaseValue": false, "overrideSeconds": 2 }),
            ),
            Some(704),
        ),
        (
            "no value",
            strength(json!({ "id": "", "setBaseValue": true, "overrideSeconds": 2 })),
            Some(705),
        ),
        (
            "no overrides",
            json!({ "strengthOverrides": [], "windOverrides": [] }),
            Some(703),
        ),
    ];
    for (what, data, expected) in cases {
        let reply = call(&mut socket, "SetCurrentModelPhysicsRequest", data).await;

        assert_eq!(error_id(&reply), expected, "{what}");
    }
}

#[tokio::test]
async fn model_requests_without_a_loaded_model_are_refused_with_their_error_ids() {
    let fake = start().await;
    let mut socket = authed(&fake).await;
    fake.unload_model().unwrap();
    let cases = [
        (
            "MoveModelRequest",
            json!({ "timeInSeconds": 0.0, "valuesAreRelativeToModel": false }),
            300,
        ),
        ("ColorTintRequest", json!({}), 250),
        ("SetCurrentModelPhysicsRequest", json!({}), 700),
    ];
    for (request, data, expected) in cases {
        let reply = call(&mut socket, request, data).await;

        assert_eq!(error_id(&reply), Some(expected), "{request}");
    }
    let current = call(&mut socket, "CurrentModelRequest", json!({})).await;
    assert_eq!(current["data"]["modelLoaded"], false);
    assert_eq!(current["data"]["modelName"], "");
}

#[tokio::test]
async fn studio_changes_that_do_not_apply_to_the_current_state_are_refused() {
    let fake = start().await;
    fake.add_item(STAR).unwrap();
    fake.set_expression(BLUSH, true).unwrap();
    let refusals = [
        ("face already found", fake.set_face_found(true)),
        ("unknown hotkey", fake.trigger_hotkey("Dance")),
        ("model already loaded", fake.load_model(AVATAR)),
        ("unknown model", fake.load_model("Ghost")),
        ("expression already on", fake.set_expression(BLUSH, true)),
        ("unknown item file", fake.add_item("nope.png")),
    ];
    for (what, outcome) in refusals {
        assert!(
            matches!(outcome, Err(EmulatorError::FakeVTubeRefused { .. })),
            "{what}: {outcome:?}"
        );
    }
    fake.remove_item(STAR).unwrap();
    fake.unload_model().unwrap();
    assert!(fake.remove_item(STAR).is_err(), "item already removed");
    assert!(fake.unload_model().is_err(), "no model left to unload");
    assert!(fake.change_model_config().is_err(), "no model to configure");
}

#[tokio::test]
async fn an_offline_fake_refuses_connections_until_it_goes_online() {
    let fake = FakeVTube::start(FakeVTubeConfig {
        online_at_boot: false,
        ..config()
    })
    .await
    .unwrap();

    let refused = tokio_tungstenite::connect_async(fake.url()).await;
    fake.go_online().await.unwrap();
    let accepted = tokio_tungstenite::connect_async(fake.url()).await;

    assert!(refused.is_err());
    assert!(accepted.is_ok());
    assert!(fake.go_online().await.is_err(), "already online");
}

#[tokio::test]
async fn invalid_configurations_are_refused_before_listening() {
    let model = |name: &str, hotkeys: &[&str], expressions: &[&str]| FakeModel {
        name: name.to_owned(),
        hotkeys: hotkeys.iter().map(|h| (*h).to_owned()).collect(),
        expressions: expressions.iter().map(|e| (*e).to_owned()).collect(),
    };
    let cases = [
        (
            "blank token",
            FakeVTubeConfig {
                token: " ".to_owned(),
                ..config()
            },
        ),
        (
            "token of 65 chars",
            FakeVTubeConfig {
                token: "t".repeat(65),
                ..config()
            },
        ),
        (
            "repeated model",
            FakeVTubeConfig {
                models: vec![model("A", &[], &[]), model("A", &[], &[])],
                ..config()
            },
        ),
        (
            "repeated hotkey",
            FakeVTubeConfig {
                models: vec![model("A", &["Wave", "Wave"], &[])],
                ..config()
            },
        ),
        (
            "expression without suffix",
            FakeVTubeConfig {
                models: vec![model("A", &[], &["Blush.json"])],
                ..config()
            },
        ),
        (
            "unknown current model",
            FakeVTubeConfig {
                current_model: Some("Ghost".to_owned()),
                ..config()
            },
        ),
        (
            "blank item",
            FakeVTubeConfig {
                items: vec![String::new()],
                ..config()
            },
        ),
    ];
    for (what, invalid) in cases {
        assert!(
            matches!(
                FakeVTube::start(invalid).await,
                Err(EmulatorError::InvalidFakeConfig { .. })
            ),
            "{what}"
        );
    }
    let longest_token = FakeVTubeConfig {
        token: "t".repeat(64),
        ..config()
    };
    assert!(FakeVTube::start(longest_token).await.is_ok());
}
