use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_platform_core::PlatformError;
use forge_types::OAuthToken;
use futures_util::StreamExt;
use rand::TryRng;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::reconnect;

const CHAT_TOKEN_ENDPOINT: &str = "https://open-api.trovo.live/openplatform/chat/token";
const CHAT_WS_ENDPOINT: &str = "wss://open-chat.trovo.live/chat";
const DEFAULT_PING_INTERVAL: Duration = Duration::from_secs(30);

pub struct TrovoChat {
    access_token: OAuthToken,
    client_id: String,
    http_client: reqwest::Client,
    chat_token_endpoint: String,
    ws_endpoint: String,
}

pub struct TrovoChatHandle {
    pub close_tx: oneshot::Sender<()>,
}

impl TrovoChat {
    pub fn new(access_token: OAuthToken, client_id: String, http_client: reqwest::Client) -> Self {
        Self {
            access_token,
            client_id,
            http_client,
            chat_token_endpoint: CHAT_TOKEN_ENDPOINT.to_owned(),
            ws_endpoint: CHAT_WS_ENDPOINT.to_owned(),
        }
    }

    pub async fn connect(
        self,
        event_tx: mpsc::Sender<Event>,
    ) -> Result<TrovoChatHandle, PlatformError> {
        let chat_token = get_chat_token(
            &self.http_client,
            self.access_token.expose(),
            &self.client_id,
            &self.chat_token_endpoint,
        )
        .await?;

        let ws_stream = connect_ws(&self.ws_endpoint).await?;
        let (close_tx, close_rx) = oneshot::channel();

        tokio::spawn(run_loop(
            ws_stream,
            chat_token,
            event_tx,
            close_rx,
            self.ws_endpoint.clone(),
            self.http_client.clone(),
            self.access_token.clone(),
            self.client_id.clone(),
            self.chat_token_endpoint.clone(),
        ));

        Ok(TrovoChatHandle { close_tx })
    }
}

async fn get_chat_token(
    client: &reqwest::Client,
    access_token: &str,
    client_id: &str,
    endpoint: &str,
) -> Result<String, PlatformError> {
    let response = client
        .get(endpoint)
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .header("client-id", client_id)
        .send()
        .await
        .map_err(|e| PlatformError::Network {
            reason: e.to_string(),
        })?;

    let status = response.status().as_u16();
    if status != 200 {
        let body = response.text().await.unwrap_or_default();
        return Err(PlatformError::Http { status, body });
    }

    let body: ChatTokenResponse = response.json().await.map_err(|e| PlatformError::Network {
        reason: e.to_string(),
    })?;

    Ok(body.token)
}

async fn connect_ws(
    ws_endpoint: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    PlatformError,
> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(ws_endpoint)
        .await
        .map_err(|e| PlatformError::Network {
            reason: e.to_string(),
        })?;
    Ok(ws_stream)
}

#[allow(clippy::too_many_arguments)]
async fn run_loop(
    mut ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    chat_token: String,
    event_tx: mpsc::Sender<Event>,
    mut close_rx: oneshot::Receiver<()>,
    ws_endpoint: String,
    http_client: reqwest::Client,
    access_token: OAuthToken,
    client_id: String,
    chat_token_endpoint: String,
) {
    let mut attempt: u32 = 0;

    if let Err(e) = send_auth(&mut ws_stream, &chat_token).await {
        warn!(error = %e, "chat AUTH send failed");
    }

    let mut ping_interval = DEFAULT_PING_INTERVAL;
    let mut ping_deadline = tokio::time::Instant::now() + ping_interval;

    loop {
        tokio::select! {
            _ = &mut close_rx => {
                info!("trovo chat close requested");
                return;
            }

            _ = tokio::time::sleep_until(ping_deadline) => {
                let nonce = random_nonce();
                let ping = serde_json::json!({"type":"PING","nonce":nonce});
                if let Err(e) = send_json(&mut ws_stream, &ping).await {
                    warn!(error = %e, "PING send failed; reconnecting");
                    break;
                }
                ping_deadline = tokio::time::Instant::now() + ping_interval;
            }

            msg = ws_stream.next() => {
                match msg {
                    None | Some(Err(_)) => {
                        warn!("trovo chat WebSocket closed; reconnecting");
                        break;
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Some(new_gap) = handle_ws_text(&text, &event_tx).await {
                            ping_interval = Duration::from_secs(new_gap);
                            ping_deadline = tokio::time::Instant::now() + ping_interval;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("trovo chat server sent close frame; reconnecting");
                        break;
                    }
                    Some(Ok(_)) => {}
                }
            }
        }
    }

    attempt += 1;
    reconnect::wait(attempt.saturating_sub(1)).await;

    loop {
        if matches!(
            close_rx.try_recv(),
            Ok(()) | Err(oneshot::error::TryRecvError::Closed)
        ) {
            return;
        }

        let new_token = match get_chat_token(
            &http_client,
            access_token.expose(),
            &client_id,
            &chat_token_endpoint,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                warn!(error = %e, "failed to get chat token on reconnect");
                reconnect::wait(attempt).await;
                attempt += 1;
                continue;
            }
        };

        let Ok(new_ws) = connect_ws(&ws_endpoint).await else {
            warn!("trovo chat WS reconnect failed");
            reconnect::wait(attempt).await;
            attempt += 1;
            continue;
        };

        ws_stream = new_ws;
        if let Err(e) = send_auth(&mut ws_stream, &new_token).await {
            warn!(error = %e, "chat AUTH send failed on reconnect");
        }

        ping_interval = DEFAULT_PING_INTERVAL;
        ping_deadline = tokio::time::Instant::now() + ping_interval;

        attempt = 0;

        loop {
            tokio::select! {
                _ = &mut close_rx => {
                    info!("trovo chat close requested");
                    return;
                }

                _ = tokio::time::sleep_until(ping_deadline) => {
                    let nonce = random_nonce();
                    let ping = serde_json::json!({"type":"PING","nonce":nonce});
                    if let Err(e) = send_json(&mut ws_stream, &ping).await {
                        warn!(error = %e, "PING send failed; reconnecting");
                        break;
                    }
                    ping_deadline = tokio::time::Instant::now() + ping_interval;
                }

                msg = ws_stream.next() => {
                    match msg {
                        None | Some(Err(_)) => {
                            warn!("trovo chat WebSocket closed; reconnecting");
                            break;
                        }
                        Some(Ok(Message::Text(text))) => {
                            if let Some(new_gap) = handle_ws_text(&text, &event_tx).await {
                                ping_interval = Duration::from_secs(new_gap);
                                ping_deadline = tokio::time::Instant::now() + ping_interval;
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("trovo chat server sent close frame; reconnecting");
                            break;
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }

        reconnect::wait(attempt).await;
        attempt += 1;
    }
}

async fn send_auth(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    chat_token: &str,
) -> Result<(), PlatformError> {
    let nonce = random_nonce();
    let auth = serde_json::json!({
        "type": "AUTH",
        "nonce": nonce,
        "data": { "token": chat_token }
    });
    send_json(ws, &auth).await
}

async fn send_json(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    value: &serde_json::Value,
) -> Result<(), PlatformError> {
    use futures_util::SinkExt;
    let text = serde_json::to_string(value).map_err(|e| PlatformError::Network {
        reason: format!("failed to serialize WS message: {e}"),
    })?;
    ws.send(Message::Text(text.into()))
        .await
        .map_err(|e| PlatformError::Network {
            reason: e.to_string(),
        })
}

async fn handle_ws_text(text: &str, event_tx: &mpsc::Sender<Event>) -> Option<u64> {
    let frame: WsChatFrame = match serde_json::from_str(text) {
        Ok(f) => f,
        Err(e) => {
            debug!(error = %e, "unrecognised WS frame; skipping");
            return None;
        }
    };

    let mut new_gap = None;

    match frame.frame_type.as_str() {
        "RESPONSE" => {
            new_gap = frame
                .data
                .as_ref()
                .and_then(|d| d.get("gap"))
                .and_then(|v| v.as_u64());
        }
        "CHAT" => {
            if let Some(chats) = frame
                .data
                .as_ref()
                .and_then(|d| d.get("chats"))
                .and_then(|v| v.as_array())
            {
                for item in chats {
                    if let Some(event) = build_event_from_item(item)
                        && event_tx.send(event).await.is_err()
                    {
                        debug!("trovo chat event receiver dropped");
                    }
                }
            }
        }
        other => {
            debug!(frame_type = %other, "unhandled trovo WS frame type");
        }
    }

    new_gap
}

pub(crate) fn classify_chat_type(type_code: u64) -> Option<&'static str> {
    match type_code {
        0 => Some("trovo.chat"),
        5 | 5009 => Some("trovo.spell"),
        5001 => Some("trovo.subscription"),
        5003 => Some("trovo.follow"),
        5004 => None,
        5005 | 5006 => Some("trovo.gift_sub"),
        _ => None,
    }
}

pub(crate) fn build_event_from_item(item: &serde_json::Value) -> Option<Event> {
    let type_code = item.get("type").and_then(|v| v.as_u64())?;
    let kind = classify_chat_type(type_code)?;

    let content = item
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let nick_name = item
        .get("nick_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let user_name = item
        .get("user_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let sender_id = item
        .get("sender_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();

    Some(Event::new(
        EventSource::Trovo,
        kind,
        serde_json::json!({
            "content": content,
            "nick_name": nick_name,
            "user_name": user_name,
            "sender_id": sender_id,
        }),
    ))
}

fn random_nonce() -> String {
    let mut bytes = [0u8; 8];
    match rand::rng().try_fill_bytes(&mut bytes) {
        Ok(()) => {}
        Err(never) => match never {},
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Deserialize)]
struct WsChatFrame {
    #[serde(rename = "type")]
    frame_type: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ChatTokenResponse {
    token: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use forge_events::EventSource;

    #[test]
    fn classify_type_0_is_chat() {
        assert_eq!(classify_chat_type(0), Some("trovo.chat"));
    }

    #[test]
    fn classify_type_5_is_spell() {
        assert_eq!(classify_chat_type(5), Some("trovo.spell"));
    }

    #[test]
    fn classify_type_5009_is_spell() {
        assert_eq!(classify_chat_type(5009), Some("trovo.spell"));
    }

    #[test]
    fn classify_type_5001_is_subscription() {
        assert_eq!(classify_chat_type(5001), Some("trovo.subscription"));
    }

    #[test]
    fn classify_type_5003_is_follow() {
        assert_eq!(classify_chat_type(5003), Some("trovo.follow"));
    }

    #[test]
    fn classify_type_5004_is_dropped() {
        assert_eq!(classify_chat_type(5004), None);
    }

    #[test]
    fn classify_type_5005_is_gift_sub() {
        assert_eq!(classify_chat_type(5005), Some("trovo.gift_sub"));
    }

    #[test]
    fn classify_type_5006_is_gift_sub() {
        assert_eq!(classify_chat_type(5006), Some("trovo.gift_sub"));
    }

    #[test]
    fn classify_unknown_type_returns_none() {
        assert_eq!(classify_chat_type(9999), None);
    }

    #[test]
    fn build_event_from_chat_item_type_0() {
        let item = json!({
            "type": 0,
            "content": "hello world",
            "nick_name": "Streamer",
            "user_name": "streamer_login",
            "sender_id": "uid_123"
        });
        let event = build_event_from_item(&item).unwrap();
        assert_eq!(event.kind, "trovo.chat");
        assert_eq!(event.source, EventSource::Trovo);
        assert_eq!(event.payload["content"].as_str(), Some("hello world"));
        assert_eq!(event.payload["nick_name"].as_str(), Some("Streamer"));
        assert_eq!(event.payload["user_name"].as_str(), Some("streamer_login"));
        assert_eq!(event.payload["sender_id"].as_str(), Some("uid_123"));
    }

    #[test]
    fn build_event_from_subscription_item() {
        let item = json!({
            "type": 5001,
            "content": "Tier 1",
            "nick_name": "NewSub",
            "user_name": "newsub_login",
            "sender_id": "uid_sub"
        });
        let event = build_event_from_item(&item).unwrap();
        assert_eq!(event.kind, "trovo.subscription");
        assert_eq!(event.source, EventSource::Trovo);
    }

    #[test]
    fn build_event_from_follow_item() {
        let item = json!({
            "type": 5003,
            "content": "",
            "nick_name": "NewFollower",
            "user_name": "follower_login",
            "sender_id": "uid_follow"
        });
        let event = build_event_from_item(&item).unwrap();
        assert_eq!(event.kind, "trovo.follow");
    }

    #[test]
    fn build_event_from_join_item_returns_none() {
        let item = json!({
            "type": 5004,
            "content": "",
            "nick_name": "Viewer",
            "user_name": "viewer",
            "sender_id": "uid_viewer"
        });
        assert!(build_event_from_item(&item).is_none());
    }

    #[test]
    fn build_event_from_gift_sub_item() {
        let item = json!({
            "type": 5005,
            "content": "1",
            "nick_name": "Gifter",
            "user_name": "gifter_login",
            "sender_id": "uid_gifter"
        });
        let event = build_event_from_item(&item).unwrap();
        assert_eq!(event.kind, "trovo.gift_sub");
    }

    #[test]
    fn build_event_from_spell_item() {
        let item = json!({
            "type": 5,
            "content": "FieryDragon",
            "nick_name": "Caster",
            "user_name": "caster_login",
            "sender_id": "uid_caster"
        });
        let event = build_event_from_item(&item).unwrap();
        assert_eq!(event.kind, "trovo.spell");
    }

    #[test]
    fn build_event_from_missing_type_returns_none() {
        let item = json!({"content": "hello"});
        assert!(build_event_from_item(&item).is_none());
    }

    #[test]
    fn random_nonce_is_16_hex_chars() {
        let nonce = random_nonce();
        assert_eq!(nonce.len(), 16);
        assert!(nonce.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn random_nonces_are_distinct() {
        let a = random_nonce();
        let b = random_nonce();
        assert_ne!(
            a, b,
            "nonces should be distinct with overwhelming probability"
        );
    }

    #[tokio::test]
    async fn get_chat_token_returns_token_on_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chat/token"))
            .and(header("client-id", "test_cid"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"token": "ws_chat_token_abc"})),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let endpoint = format!("{}/chat/token", server.uri());
        let token = get_chat_token(&client, "access_tok", "test_cid", &endpoint)
            .await
            .unwrap();
        assert_eq!(token, "ws_chat_token_abc");
    }

    #[tokio::test]
    async fn get_chat_token_returns_http_error_on_non_200() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/chat/token"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let endpoint = format!("{}/chat/token", server.uri());
        let err = get_chat_token(&client, "bad_token", "cid", &endpoint)
            .await
            .unwrap_err();
        assert!(
            matches!(err, PlatformError::Http { status: 401, .. }),
            "expected Http 401, got: {err}"
        );
    }

    #[tokio::test]
    async fn handle_ws_text_parses_response_gap() {
        let (tx, _rx) = mpsc::channel(8);
        let text =
            r#"{"type":"RESPONSE","nonce":"abc","data":{"error":"","type":"PING","gap":45}}"#;
        let gap = handle_ws_text(text, &tx).await;
        assert_eq!(gap, Some(45));
    }

    #[tokio::test]
    async fn handle_ws_text_chat_frame_sends_events() {
        let (tx, mut rx) = mpsc::channel(8);
        let text = r#"{"type":"CHAT","channel_info":{"channel_id":"12345"},"data":{"chats":[{"type":0,"content":"hi","nick_name":"User","user_name":"user_login","sender_id":"uid_1"}]}}"#;
        let gap = handle_ws_text(text, &tx).await;
        assert_eq!(gap, None);

        let event = rx.recv().await.unwrap();
        assert_eq!(event.kind, "trovo.chat");
        assert_eq!(event.payload["content"].as_str(), Some("hi"));
    }

    #[tokio::test]
    async fn handle_ws_text_ignores_join_type() {
        let (tx, mut rx) = mpsc::channel(8);
        let text = r#"{"type":"CHAT","data":{"chats":[{"type":5004,"content":"","nick_name":"V","user_name":"v","sender_id":"uid"}]}}"#;
        handle_ws_text(text, &tx).await;

        assert!(
            rx.try_recv().is_err(),
            "join events must not produce a message"
        );
    }

    #[tokio::test]
    async fn handle_ws_text_unknown_frame_produces_no_event() {
        let (tx, mut rx) = mpsc::channel(8);
        let text = r#"{"type":"UNKNOWN_FRAME","data":null}"#;
        let gap = handle_ws_text(text, &tx).await;
        assert_eq!(gap, None);
        assert!(rx.try_recv().is_err());
    }
}
