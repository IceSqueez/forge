#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use forge_emulator::EmulatorError;
use forge_emulator::fixture::TwitchAccount;
use forge_emulator::twitch::{CredentialCheck, FakeTwitch, FakeTwitchConfig, Viewer, ViewerBadge};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const DEADLINE: Duration = Duration::from_secs(5);
const CHAT: &str = "channel.chat.message";

fn config() -> FakeTwitchConfig {
    FakeTwitchConfig::for_account(&TwitchAccount::default())
}

async fn start() -> FakeTwitch {
    FakeTwitch::start(config()).await.unwrap()
}

async fn next_frame(socket: &mut Socket) -> Value {
    loop {
        let message = timeout(DEADLINE, socket.next())
            .await
            .expect("frame in time")
            .expect("socket open")
            .expect("readable frame");
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

async fn connect_to(url: &str) -> (Socket, String) {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    let welcome = next_frame(&mut socket).await;
    assert_eq!(welcome["metadata"]["message_type"], "session_welcome");
    let session_id = welcome["payload"]["session"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (socket, session_id)
}

fn subscription_body(subscription_type: &str, session_id: &str) -> Value {
    json!({
        "type": subscription_type,
        "version": "1",
        "condition": { "broadcaster_user_id": "100000001", "user_id": "100000001" },
        "transport": { "method": "websocket", "session_id": session_id },
    })
}

fn authorized(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: String,
) -> reqwest::RequestBuilder {
    let account = TwitchAccount::default();
    client
        .request(method, url)
        .header("Authorization", format!("Bearer {}", account.access_token))
        .header("Client-Id", account.client_id)
}

async fn post_json(fake: &FakeTwitch, path: &str, body: &Value) -> (u16, Value) {
    let response = authorized(
        &reqwest::Client::new(),
        reqwest::Method::POST,
        format!("{}{path}", fake.api_base_url()),
    )
    .json(body)
    .send()
    .await
    .unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap())
}

async fn subscribe(fake: &FakeTwitch, subscription_type: &str, session_id: &str) -> (u16, Value) {
    post_json(
        fake,
        "/helix/eventsub/subscriptions",
        &subscription_body(subscription_type, session_id),
    )
    .await
}

#[tokio::test]
async fn welcome_names_a_session_the_ledger_records_as_live() {
    let fake = start().await;

    let (mut socket, _) = tokio_tungstenite::connect_async(fake.eventsub_ws_url())
        .await
        .unwrap();
    let welcome = next_frame(&mut socket).await;

    assert_eq!(welcome["metadata"]["message_type"], "session_welcome");
    let session_id = welcome["payload"]["session"]["id"].as_str().unwrap();
    let ledger = fake.ledger();
    assert!(
        ledger
            .live_sessions()
            .any(|session| session.id == session_id && session.reconnected_from.is_none()),
        "welcomed session missing from {:?}",
        ledger.sessions
    );
}

#[tokio::test]
async fn idle_socket_sends_keepalives_after_the_welcome() {
    let mut config = config();
    config.keepalive_interval = Duration::from_millis(20);
    let fake = FakeTwitch::start(config).await.unwrap();

    let (mut socket, _) = connect_to(fake.eventsub_ws_url()).await;

    for _ in 0..2 {
        assert_eq!(
            next_frame(&mut socket).await["metadata"]["message_type"],
            "session_keepalive"
        );
    }
}

#[tokio::test]
async fn zero_keepalive_interval_is_refused_before_listening() {
    let mut config = config();
    config.keepalive_interval = Duration::ZERO;

    let refusal = FakeTwitch::start(config).await;

    assert!(
        matches!(refusal, Err(EmulatorError::InvalidFakeConfig { .. })),
        "got {:?}",
        refusal.err()
    );
}

#[tokio::test]
async fn accepted_subscription_is_answered_and_recorded_with_its_condition() {
    let fake = start().await;
    let (_socket, session_id) = connect_to(fake.eventsub_ws_url()).await;

    let (status, body) = subscribe(&fake, CHAT, &session_id).await;

    assert_eq!(status, 202);
    let created = &body["data"][0];
    assert_eq!(created["type"], CHAT);
    assert_eq!(created["transport"]["session_id"], session_id.as_str());
    let ledger = fake.ledger();
    assert_eq!(ledger.subscriptions.len(), 1);
    let recorded = &ledger.subscriptions[0];
    assert_eq!(created["id"], recorded.id.as_str());
    assert_eq!(recorded.session_id, session_id);
    assert_eq!(recorded.subscription_type, CHAT);
    assert_eq!(
        recorded.condition,
        subscription_body(CHAT, &session_id)["condition"]
    );
}

#[tokio::test]
async fn refused_credentials_answer_401_and_create_no_subscription() {
    let account = TwitchAccount::default();
    let bearer = format!("Bearer {}", account.access_token);
    let cases: [(Option<String>, Option<&str>, CredentialCheck); 5] = [
        (
            None,
            Some(&account.client_id),
            CredentialCheck::MissingBearer,
        ),
        (
            Some(account.access_token.clone()),
            Some(&account.client_id),
            CredentialCheck::MissingBearer,
        ),
        (
            Some("Bearer some-other-token".to_owned()),
            Some(&account.client_id),
            CredentialCheck::WrongBearer,
        ),
        (Some(bearer.clone()), None, CredentialCheck::MissingClientId),
        (
            Some(bearer.clone()),
            Some("otherclientid"),
            CredentialCheck::WrongClientId,
        ),
    ];
    let fake = start().await;
    let (_socket, session_id) = connect_to(fake.eventsub_ws_url()).await;
    let client = reqwest::Client::new();

    for (index, (authorization, client_id, expected)) in cases.into_iter().enumerate() {
        let mut request = client
            .post(format!(
                "{}/helix/eventsub/subscriptions",
                fake.api_base_url()
            ))
            .json(&subscription_body(CHAT, &session_id));
        if let Some(authorization) = authorization {
            request = request.header("Authorization", authorization);
        }
        if let Some(client_id) = client_id {
            request = request.header("Client-Id", client_id);
        }
        let status = request.send().await.unwrap().status().as_u16();

        assert_eq!(status, 401, "case {expected:?}");
        let ledger = fake.ledger();
        assert_eq!(ledger.requests[index].credentials, expected);
        assert!(ledger.subscriptions.is_empty(), "case {expected:?}");
    }
}

#[tokio::test]
async fn malformed_bodies_on_modeled_endpoints_answer_400() {
    let fake = start().await;
    let (_socket, session_id) = connect_to(fake.eventsub_ws_url()).await;
    let subscriptions = "/helix/eventsub/subscriptions";
    let mut webhook = subscription_body(CHAT, &session_id);
    webhook["transport"] = json!({ "method": "webhook", "callback": "https://192.0.2.1/" });
    let mut no_type = subscription_body(CHAT, &session_id);
    no_type.as_object_mut().unwrap().remove("type");
    let mut scalar_condition = subscription_body(CHAT, &session_id);
    scalar_condition["condition"] = json!("100000001");
    let cases = [
        (subscriptions, json!("not an object")),
        (subscriptions, no_type),
        (subscriptions, scalar_condition),
        (subscriptions, webhook),
        (subscriptions, subscription_body(CHAT, "no-such-session")),
        (
            "/helix/chat/messages",
            json!({ "broadcaster_id": "100000001", "sender_id": "100000001" }),
        ),
    ];

    for (path, body) in cases {
        let (status, _) = post_json(&fake, path, &body).await;
        assert_eq!(status, 400, "{path} with {body}");
    }
    let ledger = fake.ledger();
    assert!(ledger.subscriptions.is_empty());
    assert_eq!(ledger.unexpected_requests().count(), 0);
}

#[tokio::test]
async fn repeating_a_subscription_on_one_session_answers_409() {
    let fake = start().await;
    let (_socket, session_id) = connect_to(fake.eventsub_ws_url()).await;

    subscribe(&fake, CHAT, &session_id).await;
    let (status, _) = subscribe(&fake, CHAT, &session_id).await;

    assert_eq!(status, 409);
    assert_eq!(fake.ledger().subscriptions.len(), 1);
}

#[tokio::test]
async fn unmodeled_requests_answer_404_and_are_flagged_unexpected() {
    let fake = start().await;
    let client = reqwest::Client::new();
    let cases = [
        (reqwest::Method::GET, "/helix/channels"),
        (reqwest::Method::DELETE, "/helix/eventsub/subscriptions"),
        (reqwest::Method::GET, "/helix/chat/messages"),
        (reqwest::Method::GET, "/"),
    ];

    for (method, path) in &cases {
        let url = format!("{}{path}?broadcaster_id=100000001", fake.api_base_url());
        let status = authorized(&client, method.clone(), url)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16();
        assert_eq!(status, 404, "{method} {path}");
    }

    let ledger = fake.ledger();
    let unexpected: Vec<_> = ledger
        .unexpected_requests()
        .map(|request| (request.method.as_str(), request.path.as_str()))
        .collect();
    let expected: Vec<_> = cases
        .iter()
        .map(|(method, path)| (method.as_str(), *path))
        .collect();
    assert_eq!(unexpected, expected);
    assert_eq!(
        ledger.requests[0].query,
        vec![("broadcaster_id".to_owned(), "100000001".to_owned())]
    );
}

#[tokio::test]
async fn injected_chat_message_arrives_as_a_channel_chat_message_notification() {
    let fake = start().await;
    let (mut socket, session_id) = connect_to(fake.eventsub_ws_url()).await;
    subscribe(&fake, CHAT, &session_id).await;
    let viewer = Viewer::new("200000042", "alice")
        .with_badge(ViewerBadge::Vip)
        .with_badge(ViewerBadge::Subscriber { months: 7 });

    let message_id = fake.inject_chat_message(&viewer, "!ping").await.unwrap();
    let frame = next_frame(&mut socket).await;

    let account = TwitchAccount::default();
    assert_eq!(frame["metadata"]["message_type"], "notification");
    assert_eq!(frame["metadata"]["subscription_type"], CHAT);
    assert_eq!(frame["metadata"]["subscription_version"], "1");
    assert_eq!(
        frame["payload"]["subscription"]["transport"]["session_id"],
        session_id.as_str()
    );
    let event = &frame["payload"]["event"];
    assert_eq!(event["message_id"], message_id.as_str());
    assert_eq!(event["broadcaster_user_id"], account.user_id.as_str());
    assert_eq!(event["broadcaster_user_login"], account.login.as_str());
    assert_eq!(event["chatter_user_id"], "200000042");
    assert_eq!(event["chatter_user_login"], "alice");
    assert_eq!(event["message"]["text"], "!ping");
    assert_eq!(event["message"]["fragments"][0]["text"], "!ping");
    assert_eq!(
        event["badges"],
        json!([
            { "set_id": "vip", "id": "1", "info": "" },
            { "set_id": "subscriber", "id": "7", "info": "7" },
        ])
    );
}

#[tokio::test]
async fn injection_without_a_subscription_is_refused() {
    let fake = start().await;
    let (_socket, _) = connect_to(fake.eventsub_ws_url()).await;

    let refusal = fake
        .inject_chat_message(&Viewer::new("200000042", "alice"), "hi")
        .await;

    assert!(
        matches!(&refusal, Err(EmulatorError::NotSubscribed { subscription_type }) if subscription_type == CHAT),
        "got {refusal:?}"
    );
}

#[tokio::test]
async fn notification_reaches_only_the_session_holding_the_subscription() {
    let fake = start().await;
    let (mut subscribed, subscribed_id) = connect_to(fake.eventsub_ws_url()).await;
    let (mut bystander, _) = connect_to(fake.eventsub_ws_url()).await;
    subscribe(&fake, CHAT, &subscribed_id).await;

    let reached = fake
        .inject_notification(CHAT, json!({ "message": { "text": "hi" } }))
        .await
        .unwrap();
    fake.inject_session_reconnect().await.unwrap();

    assert_eq!(reached, 1);
    assert_eq!(
        next_frame(&mut subscribed).await["metadata"]["message_type"],
        "notification"
    );
    assert_eq!(
        next_frame(&mut bystander).await["metadata"]["message_type"],
        "session_reconnect",
        "the bystander's first frame after the welcome must not be the notification"
    );
}

#[tokio::test]
async fn closed_session_stops_receiving_injections() {
    let fake = start().await;
    let (mut socket, session_id) = connect_to(fake.eventsub_ws_url()).await;
    subscribe(&fake, CHAT, &session_id).await;

    socket.close(None).await.unwrap();
    fake.wait_for("the session to close", DEADLINE, |ledger| {
        ledger.live_sessions().next().is_none().then_some(())
    })
    .await
    .unwrap();
    let refusal = fake
        .inject_chat_message(&Viewer::new("200000042", "alice"), "hi")
        .await;

    assert!(
        matches!(refusal, Err(EmulatorError::NotSubscribed { .. })),
        "got {refusal:?}"
    );
}

#[tokio::test]
async fn reconnect_url_opens_a_session_that_inherits_the_subscriptions() {
    let fake = start().await;
    let (mut original, original_id) = connect_to(fake.eventsub_ws_url()).await;
    subscribe(&fake, CHAT, &original_id).await;

    fake.inject_session_reconnect().await.unwrap();
    let reconnect = next_frame(&mut original).await;
    assert_eq!(reconnect["metadata"]["message_type"], "session_reconnect");
    let url = reconnect["payload"]["session"]["reconnect_url"]
        .as_str()
        .unwrap();
    let (mut successor, successor_id) = connect_to(url).await;
    fake.inject_chat_message(&Viewer::new("200000042", "alice"), "still here")
        .await
        .unwrap();

    let frame = next_frame(&mut successor).await;
    assert_eq!(frame["payload"]["event"]["message"]["text"], "still here");
    let ledger = fake.ledger();
    let session = ledger
        .sessions
        .iter()
        .find(|session| session.id == successor_id)
        .unwrap();
    assert_eq!(
        session.reconnected_from.as_deref(),
        Some(original_id.as_str())
    );
}

#[tokio::test]
async fn original_socket_closes_once_its_reconnect_session_opens() {
    let fake = start().await;
    let (mut original, _) = connect_to(fake.eventsub_ws_url()).await;

    fake.inject_session_reconnect().await.unwrap();
    let reconnect = next_frame(&mut original).await;
    let url = reconnect["payload"]["session"]["reconnect_url"]
        .as_str()
        .unwrap();
    let (_successor, _) = connect_to(url).await;

    let ended = timeout(DEADLINE, async {
        loop {
            match original.next().await {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "original socket stayed open");
    assert_eq!(fake.ledger().live_sessions().count(), 1);
}

#[tokio::test]
async fn reconnect_without_a_live_session_is_refused() {
    let fake = start().await;

    let refusal = fake.inject_session_reconnect().await;

    assert!(
        matches!(refusal, Err(EmulatorError::NoLiveSession)),
        "got {refusal:?}"
    );
}

#[tokio::test]
async fn user_lookup_resolves_the_broadcaster_and_known_viewers_only() {
    let fake = start().await;
    let (_socket, session_id) = connect_to(fake.eventsub_ws_url()).await;
    subscribe(&fake, CHAT, &session_id).await;
    fake.inject_chat_message(&Viewer::new("200000042", "alice"), "hi")
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let cases: [(&str, &[&str]); 5] = [
        ("", &["100000001"]),
        ("?id=100000001", &["100000001"]),
        ("?login=ALICE", &["200000042"]),
        ("?id=100000001&login=alice", &["100000001", "200000042"]),
        ("?login=nobody", &[]),
    ];

    for (query, expected_ids) in cases {
        let url = format!("{}/helix/users{query}", fake.api_base_url());
        let body: Value = authorized(&client, reqwest::Method::GET, url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let ids: Vec<&str> = body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|user| user["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, expected_ids, "query {query:?}");
    }
}

#[tokio::test]
async fn session_holding_two_subscriptions_of_one_type_gets_one_notification() {
    let fake = start().await;
    let (mut socket, session_id) = connect_to(fake.eventsub_ws_url()).await;
    for condition in [
        json!({ "to_broadcaster_user_id": "100000001" }),
        json!({ "from_broadcaster_user_id": "100000001" }),
    ] {
        let mut body = subscription_body("channel.raid", &session_id);
        body["condition"] = condition;
        post_json(&fake, "/helix/eventsub/subscriptions", &body).await;
    }

    let reached = fake
        .inject_notification("channel.raid", json!({ "viewers": 5 }))
        .await
        .unwrap();
    fake.inject_session_reconnect().await.unwrap();

    assert_eq!(reached, 1);
    assert_eq!(
        next_frame(&mut socket).await["metadata"]["message_type"],
        "notification"
    );
    assert_eq!(
        next_frame(&mut socket).await["metadata"]["message_type"],
        "session_reconnect"
    );
}

#[tokio::test]
async fn shutdown_closes_open_sockets_and_stops_accepting() {
    let fake = start().await;
    let (mut socket, session_id) = connect_to(fake.eventsub_ws_url()).await;
    subscribe(&fake, CHAT, &session_id).await;
    let socket_url = fake.eventsub_ws_url().to_owned();
    let api_url = format!("{}/helix/streams", fake.api_base_url());

    timeout(DEADLINE, fake.shutdown())
        .await
        .expect("shutdown finishes within its grace period");

    let ended = timeout(DEADLINE, async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert!(ended.is_ok(), "open socket survived shutdown");
    assert!(tokio_tungstenite::connect_async(socket_url).await.is_err());
    assert!(reqwest::get(api_url).await.is_err());
}
