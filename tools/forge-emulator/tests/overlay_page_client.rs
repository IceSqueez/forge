//! The page client against a stub that speaks forge's overlay contract. Nothing here starts the
//! real forge binary.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use axum::routing::get;
use forge_emulator::EmulatorError;
use forge_emulator::overlay::{OverlayPage, PageEndpoint};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::Instant;

const DEADLINE: Duration = Duration::from_secs(5);
const IDENTITY: &str = "alert-box";
const CREDENTIAL: &str = "0123456789abcdef0123456789abcdef";
const CONTENT: &str =
    r#"{"frame":"content","content":{"headline":"alice raised an alert"},"durationMs":5000}"#;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Behaviour {
    Accept,
    AnswerThenDeliver,
    DeliverThenAnswer,
    Refuse,
}

#[derive(Default)]
struct Seen {
    config_origin: Option<String>,
    socket_origin: Option<String>,
    credential: Option<String>,
}

#[derive(Clone)]
struct StubState {
    seen: Arc<Mutex<Seen>>,
    behaviour: Behaviour,
    document: Arc<Option<String>>,
}

struct Stub {
    endpoint: PageEndpoint,
    seen: Arc<Mutex<Seen>>,
    task: JoinHandle<()>,
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Stub {
    async fn start(behaviour: Behaviour, document: Option<String>) -> Self {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let state = StubState {
            seen: Arc::clone(&seen),
            behaviour,
            document: Arc::new(document),
        };
        let router = Router::new()
            .route("/overlays/{*path}", get(serve_config))
            .route("/ws/v1/", get(upgrade))
            .with_state(state);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Self {
            endpoint: PageEndpoint::loopback(addr.port()).unwrap(),
            seen,
            task,
        }
    }

    async fn open(&self) -> Result<OverlayPage, EmulatorError> {
        OverlayPage::open(&self.endpoint, IDENTITY, DEADLINE).await
    }

    fn seen(&self) -> (Option<String>, Option<String>, Option<String>) {
        let seen = self.seen.lock().unwrap();
        (
            seen.config_origin.clone(),
            seen.socket_origin.clone(),
            seen.credential.clone(),
        )
    }
}

fn default_document() -> String {
    json!({
        "documentVersion": 1,
        "overlayId": IDENTITY,
        "credential": CREDENTIAL,
        "config": { "headline": "stored headline" },
    })
    .to_string()
}

async fn serve_config(State(state): State<StubState>, headers: HeaderMap) -> Response {
    state.seen.lock().unwrap().config_origin = origin_of(&headers);
    match state.document.as_ref() {
        Some(body) => Response::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(body.clone().into())
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::empty())
            .unwrap(),
    }
}

async fn upgrade(
    upgrade: WebSocketUpgrade,
    State(state): State<StubState>,
    headers: HeaderMap,
) -> Response {
    state.seen.lock().unwrap().socket_origin = origin_of(&headers);
    upgrade.on_upgrade(move |socket| converse(socket, state))
}

fn origin_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

async fn converse(mut socket: WebSocket, state: StubState) {
    let Some(Ok(Message::Text(text))) = socket.recv().await else {
        return;
    };
    let request: Value = serde_json::from_str(&text).unwrap();
    state.seen.lock().unwrap().credential = request
        .get("overlayCredential")
        .and_then(Value::as_str)
        .map(str::to_owned);

    let answer = match state.behaviour {
        Behaviour::Refuse => json!({
            "id": "1",
            "status": "error",
            "error": { "code": "AUTH_FAILED", "message": "invalid credential" },
        }),
        _ => json!({ "id": "1", "status": "ok", "authenticated": true }),
    }
    .to_string();

    if state.behaviour == Behaviour::DeliverThenAnswer {
        let _ = socket.send(Message::Text(CONTENT.into())).await;
    }
    let _ = socket.send(Message::Text(answer.into())).await;
    if state.behaviour == Behaviour::AnswerThenDeliver {
        let _ = socket.send(Message::Text(CONTENT.into())).await;
    }
    if state.behaviour == Behaviour::Refuse {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }
    while socket.recv().await.is_some() {}
}

async fn first_frame(page: &OverlayPage) -> Value {
    page.wait_until(Instant::now() + DEADLINE, |frames| !frames.is_empty())
        .await;
    page.read(|frames| {
        frames
            .first()
            .map(|frame| frame.raw.clone())
            .expect("a frame arrived before the deadline")
    })
}

#[tokio::test]
async fn a_page_presents_the_credential_its_own_config_document_carries() {
    let stub = Stub::start(Behaviour::Accept, Some(default_document())).await;

    let page = stub.open().await.expect("forge accepted the page");

    let (config_origin, socket_origin, credential) = stub.seen();
    assert_eq!(page.identity(), IDENTITY);
    assert_eq!(credential.as_deref(), Some(CREDENTIAL));
    let origin = stub.endpoint.origin();
    assert_eq!(
        (config_origin.as_deref(), socket_origin.as_deref()),
        (Some(origin.as_str()), Some(origin.as_str())),
        "a page forge would serve is always same-origin; a client that omits Origin never exercises the check"
    );
}

#[tokio::test]
async fn a_content_frame_is_recorded_exactly_as_it_came_off_the_wire() {
    let stub = Stub::start(Behaviour::AnswerThenDeliver, Some(default_document())).await;
    let page = stub.open().await.expect("forge accepted the page");

    let frame = first_frame(&page).await;

    assert_eq!(
        frame,
        serde_json::from_str::<Value>(CONTENT).unwrap(),
        "a report is only evidence while the frame reaches it unaltered"
    );
}

#[tokio::test]
async fn content_delivered_before_the_credential_is_answered_is_still_recorded() {
    let stub = Stub::start(Behaviour::DeliverThenAnswer, Some(default_document())).await;
    let page = stub.open().await.expect("forge accepted the page");

    let frame = first_frame(&page).await;

    assert_eq!(
        frame["content"]["headline"],
        json!("alice raised an alert"),
        "forge replays retained content from inside the credential check, so a page that only starts listening afterwards would miss a replay"
    );
}

#[tokio::test]
async fn a_page_that_cannot_be_opened_says_which_half_of_the_contract_failed() {
    for (behaviour, document, expected, label) in [
        (
            Behaviour::Accept,
            None,
            "answered 404",
            "an overlay whose files were never written",
        ),
        (
            Behaviour::Accept,
            Some(json!({ "overlayId": IDENTITY, "config": {} }).to_string()),
            "carries no page credential",
            "a config document that names no credential",
        ),
        (
            Behaviour::Accept,
            Some(json!({ "credential": "", "config": {} }).to_string()),
            "carries no page credential",
            "a config document whose credential is empty",
        ),
        (
            Behaviour::Refuse,
            Some(default_document()),
            "invalid credential",
            "a credential forge no longer honours",
        ),
    ] {
        let stub = Stub::start(behaviour, document).await;

        let refusal = stub.open().await;

        let rendered = match &refusal {
            Err(e) => e.to_string(),
            Ok(_) => panic!("{label}: the page opened anyway"),
        };
        assert!(
            rendered.contains(expected),
            "{label}: `{rendered}` does not say `{expected}`"
        );
        assert!(
            rendered.contains(IDENTITY),
            "{label}: `{rendered}` does not name the overlay"
        );
    }
}
