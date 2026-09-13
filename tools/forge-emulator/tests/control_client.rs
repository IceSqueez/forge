#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use forge_emulator::EmulatorError;
use forge_emulator::control::{
    ClientTimeouts, ControlClient, ControlEndpoint, EventFilter, EventStream, Observation,
};
use forge_events::{Event, EventSource};
use forge_server::protocol::{WsEnvelope, WsRequest};
use forge_types::ActionId;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Map, Value, json};
use time::format_description::well_known::Rfc3339;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

static WILDCARD: [EventFilter; 1] = [EventFilter {
    source: None,
    kind: None,
}];

const DEADLINE: Duration = Duration::from_secs(5);
/// Long enough that the follow-up request is never the one that times out on a loaded machine.
const ABANDON_TIMEOUT: Duration = Duration::from_millis(250);

/// Scripted loopback peer: the test sees every client frame and decides every reply.
struct FakeForge {
    endpoint: ControlEndpoint,
    requests: mpsc::UnboundedReceiver<Value>,
    frames: mpsc::UnboundedSender<Option<Message>>,
}

impl FakeForge {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (requests_tx, requests) = mpsc::unbounded_channel();
        let (frames, mut frames_rx) = mpsc::unbounded_channel::<Option<Message>>();
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
                            let _ = requests_tx.send(serde_json::from_str(text.as_str()).unwrap());
                        }
                        Some(Ok(_)) => {}
                        _ => break,
                    },
                    outgoing = frames_rx.recv() => match outgoing {
                        Some(Some(frame)) => {
                            if socket.send(frame).await.is_err() {
                                break;
                            }
                        }
                        Some(None) | None => {
                            let _ = socket.close(None).await;
                            break;
                        }
                    },
                }
            }
        });
        Self {
            endpoint: ControlEndpoint::loopback("127.0.0.1", port).unwrap(),
            requests,
            frames,
        }
    }

    async fn connect(&self, request_timeout: Duration) -> (ControlClient, EventStream) {
        let timeouts = ClientTimeouts {
            connect: DEADLINE,
            request: request_timeout,
        };
        ControlClient::connect(&self.endpoint, timeouts)
            .await
            .unwrap()
    }

    async fn next_request(&mut self) -> Value {
        timeout(DEADLINE, self.requests.recv())
            .await
            .expect("client sent a request in time")
            .expect("fake forge still running")
    }

    fn send(&self, frame: Value) {
        self.frames
            .send(Some(Message::Text(frame.to_string().into())))
            .unwrap();
    }

    fn close(&self) {
        self.frames.send(None).unwrap();
    }
}

fn ok_response(request: &Value, fields: Value) -> Value {
    let mut frame = fields;
    frame["id"] = request["id"].clone();
    frame["status"] = json!("ok");
    frame
}

fn error_response(id: Value, code: &str, message: &str) -> Value {
    json!({ "id": id, "status": "error", "error": { "code": code, "message": message } })
}

fn chat_event(text: &str) -> Event {
    Event::new(
        EventSource::Twitch,
        "twitch.channel.chat.message",
        json!({ "text": text }),
    )
}

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

async fn next_observation(stream: &mut EventStream) -> Observation {
    timeout(DEADLINE, stream.next())
        .await
        .expect("observation in time")
        .expect("stream still open")
}

#[tokio::test]
async fn authenticate_presents_the_bearer_token_and_resolves_on_acceptance() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;

    let (outcome, request) = tokio::join!(client.authenticate("fixture-bearer"), async {
        let request = forge.next_request().await;
        forge.send(ok_response(&request, json!({ "authenticated": true })));
        request
    });

    outcome.expect("authentication accepted");
    assert_eq!(request["request"], "auth");
    assert_eq!(request["token"], "fixture-bearer");
}

#[tokio::test]
async fn authenticate_refusal_is_auth_refused_and_never_echoes_the_token() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;
    let token = "fixture-bearer-that-must-not-leak";

    let (outcome, ()) = tokio::join!(client.authenticate(token), async {
        let request = forge.next_request().await;
        forge.send(error_response(
            request["id"].clone(),
            "AUTH_FAILED",
            "invalid token",
        ));
    });

    let err = outcome.expect_err("refused token must not authenticate");
    assert!(
        matches!(&err, EmulatorError::AuthRefused { message } if message == "invalid token"),
        "{err:?}"
    );
    assert!(!err.to_string().contains(token));
    assert!(!format!("{err:?}").contains(token));
}

#[tokio::test]
async fn a_refused_request_carries_the_server_code_and_message() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;

    let (outcome, ()) = tokio::join!(client.subscribe(&WILDCARD), async {
        let request = forge.next_request().await;
        forge.send(error_response(
            request["id"].clone(),
            "UNAUTHENTICATED",
            "authentication required",
        ));
    });

    assert!(
        matches!(
            &outcome,
            Err(EmulatorError::Refused { request: "subscribe", code: Some(code), message })
                if code == "UNAUTHENTICATED" && message == "authentication required"
        ),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn subscribe_sends_filters_in_the_server_wire_shape() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;
    let filters = [
        EventFilter {
            source: Some(EventSource::YouTube),
            kind: Some("youtube.chat.message".to_owned()),
        },
        EventFilter::default(),
    ];

    let (outcome, request) = tokio::join!(client.subscribe(&filters), async {
        let request = forge.next_request().await;
        forge.send(ok_response(&request, json!({ "subscribed": [] })));
        request
    });

    outcome.expect("subscription accepted");
    assert_eq!(request["request"], "subscribe");
    // Why: the server widens a source spelling it cannot parse into a wildcard, so drift here
    // would silently over-subscribe instead of failing.
    assert_eq!(
        request["events"],
        json!([{ "source": "you_tube", "type": "youtube.chat.message" }, {}])
    );
}

#[tokio::test]
async fn do_action_request_decodes_as_the_servers_do_action_and_yields_the_execution_id() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;
    let action = ActionId::new();
    let args = json!({ "user": "лісоруб", "count": 3 })
        .as_object()
        .cloned()
        .unwrap();

    let (outcome, request) = tokio::join!(client.do_action(action, &args), async {
        let request = forge.next_request().await;
        forge.send(ok_response(
            &request,
            json!({ "ok": true, "execution_id": "01K4ZEXECUTION" }),
        ));
        request
    });

    assert_eq!(outcome.expect("action accepted"), "01K4ZEXECUTION");
    let decoded: WsEnvelope<WsRequest> = serde_json::from_value(request).unwrap();
    assert!(
        matches!(&decoded.inner, WsRequest::DoAction { action_id, args: sent }
            if *action_id == action.to_string() && sent == &Value::Object(args.clone())),
        "{decoded:?}"
    );
}

#[tokio::test]
async fn do_action_answer_without_an_execution_id_is_an_unexpected_response() {
    for body in [
        json!({ "ok": true }),
        json!({ "ok": true, "execution_id": 7 }),
    ] {
        let mut forge = FakeForge::start().await;
        let (client, _events) = forge.connect(DEADLINE).await;
        let no_args = Map::new();

        let (outcome, ()) = tokio::join!(client.do_action(ActionId::new(), &no_args), async {
            let request = forge.next_request().await;
            forge.send(ok_response(&request, body.clone()));
        });

        assert!(
            matches!(
                outcome,
                Err(EmulatorError::UnexpectedResponse {
                    request: "doAction",
                    ..
                })
            ),
            "body {body}"
        );
    }
}

#[tokio::test]
async fn set_global_request_decodes_as_the_servers_set_global() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;
    let value = json!({ "streak": [1, 2.5, "три"] });

    let (outcome, request) = tokio::join!(client.set_global("mood", &value, true), async {
        let request = forge.next_request().await;
        forge.send(ok_response(&request, json!({ "ok": true })));
        request
    });

    outcome.expect("global accepted");
    let decoded: WsEnvelope<WsRequest> = serde_json::from_value(request).unwrap();
    assert!(
        matches!(&decoded.inner, WsRequest::SetGlobal { name, value: sent, persisted: true }
            if name == "mood" && sent == &value),
        "{decoded:?}"
    );
}

#[tokio::test]
async fn recent_events_requests_the_limit_and_decodes_history_entries() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;
    let stored = chat_event("from history");

    let (outcome, request) = tokio::join!(client.recent_events(7), async {
        let request = forge.next_request().await;
        forge.send(ok_response(
            &request,
            json!({ "events": [serde_json::to_value(&stored).unwrap()] }),
        ));
        request
    });

    assert_eq!(request["request"], "getEvents");
    assert_eq!(request["limit"], 7);
    let events = outcome.expect("history decodes");
    assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), [stored.id]);
}

#[tokio::test]
async fn recent_events_with_a_malformed_body_is_an_unexpected_response() {
    let stored = serde_json::to_value(chat_event("x")).unwrap();
    let mut without_kind = stored.clone();
    without_kind.as_object_mut().unwrap().remove("kind");
    for body in [
        json!({}),
        json!({ "events": {} }),
        json!({ "events": [without_kind] }),
    ] {
        let mut forge = FakeForge::start().await;
        let (client, _events) = forge.connect(DEADLINE).await;

        let (outcome, ()) = tokio::join!(client.recent_events(1), async {
            let request = forge.next_request().await;
            forge.send(ok_response(&request, body.clone()));
        });

        assert!(
            matches!(
                outcome,
                Err(EmulatorError::UnexpectedResponse {
                    request: "getEvents",
                    ..
                })
            ),
            "body {body}"
        );
    }
}

#[tokio::test]
async fn responses_answered_out_of_order_reach_their_own_callers() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;
    let stored = chat_event("second request");

    let (subscribed, history, ()) = tokio::join!(
        client.subscribe(&WILDCARD),
        client.recent_events(1),
        async {
            let first = forge.next_request().await;
            let second = forge.next_request().await;
            let (subscribe, get_events) = if first["request"] == "subscribe" {
                (first, second)
            } else {
                (second, first)
            };
            forge.send(ok_response(
                &get_events,
                json!({ "events": [serde_json::to_value(&stored).unwrap()] }),
            ));
            forge.send(ok_response(&subscribe, json!({ "subscribed": [] })));
        }
    );

    subscribed.expect("subscribe answered");
    let events = history.expect("getEvents answered with its own body");
    assert_eq!(events.iter().map(|e| e.id).collect::<Vec<_>>(), [stored.id]);
}

#[tokio::test]
async fn an_uncorrelatable_error_answers_the_oldest_pending_request() {
    let mut forge = FakeForge::start().await;
    let (client, _events) = forge.connect(DEADLINE).await;

    let (outcome, ()) = tokio::join!(client.subscribe(&WILDCARD), async {
        forge.next_request().await;
        forge.send(error_response(
            Value::Null,
            "INVALID_PAYLOAD",
            "unknown variant",
        ));
    });

    assert!(
        matches!(
            &outcome,
            Err(EmulatorError::Refused { code: Some(code), .. }) if code == "INVALID_PAYLOAD"
        ),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_late_answer_to_a_timed_out_request_never_reaches_the_next_request() {
    for late_answer_is_correlated in [true, false] {
        let mut forge = FakeForge::start().await;
        let (client, _events) = forge.connect(ABANDON_TIMEOUT).await;
        let stale = chat_event("stale");
        let fresh = chat_event("fresh");

        let (abandoned, abandoned_request) =
            tokio::join!(client.recent_events(1), forge.next_request());
        assert!(
            matches!(
                abandoned,
                Err(EmulatorError::RequestTimeout {
                    request: "getEvents"
                })
            ),
            "{abandoned:?}"
        );

        let (outcome, ()) = tokio::join!(client.recent_events(1), async {
            let next_request = forge.next_request().await;
            if late_answer_is_correlated {
                forge.send(ok_response(
                    &abandoned_request,
                    json!({ "events": [serde_json::to_value(&stale).unwrap()] }),
                ));
            } else {
                forge.send(error_response(Value::Null, "INVALID_PAYLOAD", "stale"));
            }
            forge.send(ok_response(
                &next_request,
                json!({ "events": [serde_json::to_value(&fresh).unwrap()] }),
            ));
        });

        let ids = outcome
            .unwrap_or_else(|e| panic!("correlated={late_answer_is_correlated}: {e:?}"))
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, [fresh.id], "correlated={late_answer_is_correlated}");
    }
}

#[tokio::test]
async fn pushes_and_drop_notices_around_a_response_reach_the_stream_in_order() {
    let mut forge = FakeForge::start().await;
    let (client, mut events) = forge.connect(DEADLINE).await;
    let before = chat_event("before");
    let after = chat_event("after");

    let (outcome, ()) = tokio::join!(client.subscribe(&WILDCARD), async {
        let request = forge.next_request().await;
        forge.send(push_frame(&before));
        forge.send(json!({ "dropped": 3 }));
        forge.send(ok_response(&request, json!({ "subscribed": [] })));
        forge.send(push_frame(&after));
    });

    outcome.expect("subscribe answered despite interleaved pushes");
    assert!(
        matches!(next_observation(&mut events).await, Observation::Event(e) if e.id == before.id)
    );
    assert!(matches!(
        next_observation(&mut events).await,
        Observation::Dropped(3)
    ));
    assert!(
        matches!(next_observation(&mut events).await, Observation::Event(e) if e.id == after.id)
    );
}

#[tokio::test]
async fn server_close_fails_the_pending_request_and_ends_the_event_stream() {
    let mut forge = FakeForge::start().await;
    let (client, mut events) = forge.connect(Duration::from_secs(30)).await;

    let (outcome, ()) = timeout(DEADLINE, async {
        tokio::join!(client.subscribe(&WILDCARD), async {
            forge.next_request().await;
            forge.close();
        })
    })
    .await
    .expect("close is noticed well before the request timeout");

    assert!(
        matches!(outcome, Err(EmulatorError::ConnectionClosed)),
        "{outcome:?}"
    );
    assert!(
        timeout(DEADLINE, events.next())
            .await
            .expect("stream ends in time")
            .is_none()
    );
}

#[tokio::test]
async fn requests_after_the_connection_closed_fail_without_waiting() {
    let forge = FakeForge::start().await;
    let (client, mut events) = forge.connect(Duration::from_secs(30)).await;
    forge.close();
    assert!(
        timeout(DEADLINE, events.next())
            .await
            .expect("stream ends in time")
            .is_none()
    );

    let outcome = timeout(DEADLINE, client.subscribe(&WILDCARD))
        .await
        .expect("closed client answers immediately");

    assert!(
        matches!(outcome, Err(EmulatorError::ConnectionClosed)),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn a_refused_upgrade_is_a_connect_error() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let _ = stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\n\r\n")
            .await;
    });
    let endpoint = ControlEndpoint::loopback("127.0.0.1", port).unwrap();

    let outcome = ControlClient::connect(&endpoint, ClientTimeouts::default()).await;

    assert!(
        matches!(outcome, Err(EmulatorError::Connect { .. })),
        "connect succeeded against a refused upgrade"
    );
}

#[tokio::test]
async fn a_peer_that_never_answers_the_upgrade_is_a_connect_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = ControlEndpoint::loopback("127.0.0.1", port).unwrap();
    let timeouts = ClientTimeouts {
        connect: Duration::from_millis(20),
        request: DEADLINE,
    };

    let outcome = ControlClient::connect(&endpoint, timeouts).await;

    assert!(
        matches!(outcome, Err(EmulatorError::ConnectTimeout)),
        "connect did not time out"
    );
    drop(listener);
}
