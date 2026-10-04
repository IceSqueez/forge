#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_platform_core::{
    ChatPlatform, EndpointSurface, PlatformEndpoints, PlatformError, RateLimitOutcome, RateLimiter,
};
use forge_platform_kick::chat::PUSHER_APP_KEY;
use forge_platform_kick::{
    ChannelInfoFetcher, KickAuthBundle, KickCategories, KickChannel, KickChat,
    KickCredentialsManager, KickModeration, KickPlatform, KickRewards, KickSendChat,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use futures_util::StreamExt;
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const STEP_DEADLINE: Duration = Duration::from_secs(5);
const CHATROOM_ID: u64 = 4242;

struct GrantLimiter;

#[async_trait]
impl RateLimiter for GrantLimiter {
    async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
        Ok(RateLimitOutcome::Granted)
    }

    fn remaining(&self) -> u32 {
        u32::MAX
    }

    async fn observe_remote_throttle(&self, _retry_after: Duration) {}
}

#[derive(Default)]
struct MemoryRepo {
    rows: Mutex<BTreeMap<String, String>>,
}

#[async_trait]
impl CredentialsRepo for MemoryRepo {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        self.rows
            .lock()
            .unwrap()
            .insert(id.to_string(), plaintext_bundle.to_owned());
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.rows.lock().unwrap().get(&id.to_string()).cloned())
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self.rows.lock().unwrap().remove(&id.to_string()).is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(Vec::new())
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

fn endpoints(overrides: &[(EndpointSurface, String)]) -> PlatformEndpoints {
    let table: BTreeMap<&'static str, String> = overrides
        .iter()
        .map(|(surface, url)| (surface.env_var(), url.clone()))
        .collect();
    PlatformEndpoints::resolve(|variable| table.get(variable).map(OsString::from)).unwrap()
}

fn limiter() -> Arc<dyn RateLimiter> {
    Arc::new(GrantLimiter)
}

async fn received_routes(server: &MockServer) -> Vec<(String, String)> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect()
}

#[tokio::test]
async fn public_api_override_carries_every_rest_client_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [] })))
        .mount(&server)
        .await;
    let endpoints = endpoints(&[(
        EndpointSurface::KickPublicApi,
        format!("{}/public/v1", server.uri()),
    )]);

    let sender = KickSendChat::new(&endpoints, limiter());
    sender.send("hi", "tok", 7, false).await.ok();
    sender.delete("msg-1", "tok").await.ok();
    KickModeration::new(&endpoints, limiter())
        .ban(9, 7, "tok")
        .await
        .ok();
    KickChannel::new(&endpoints, limiter())
        .get_channel("tok")
        .await
        .ok();
    KickRewards::new(&endpoints, limiter())
        .delete("reward-1", "tok")
        .await
        .ok();
    KickCategories::new(&endpoints, limiter())
        .search("tok", "chess")
        .await
        .ok();

    let expected: Vec<(String, String)> = [
        ("POST", "/public/v1/chat"),
        ("DELETE", "/public/v1/chat/msg-1"),
        ("POST", "/public/v1/moderation/bans"),
        ("GET", "/public/v1/channels"),
        ("DELETE", "/public/v1/channels/rewards/reward-1"),
        ("GET", "/public/v1/categories"),
    ]
    .into_iter()
    .map(|(verb, route)| (verb.to_owned(), route.to_owned()))
    .collect();
    assert_eq!(received_routes(&server).await, expected);
}

#[tokio::test]
async fn channel_api_override_carries_the_channel_info_fetch_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/channels/streamer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "chatroom": { "id": CHATROOM_ID },
            "livestream": { "viewer_count": 31 }
        })))
        .mount(&server)
        .await;
    let endpoints = endpoints(&[(
        EndpointSurface::KickChannelApi,
        format!("{}/api/v2", server.uri()),
    )]);

    let info = ChannelInfoFetcher::new(&endpoints, "streamer".to_owned(), reqwest::Client::new())
        .fetch()
        .await
        .unwrap();

    assert_eq!((info.chatroom_id, info.viewer_count), (CHATROOM_ID, 31));
}

#[tokio::test]
async fn oauth_override_carries_the_token_refresh_to_the_override_host() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fresh-access",
            "refresh_token": "fresh-refresh",
            "expires_in": 3600
        })))
        .mount(&server)
        .await;
    let endpoints = endpoints(&[(
        EndpointSurface::KickOAuth,
        format!("{}/oauth", server.uri()),
    )]);
    let manager = KickCredentialsManager::new(
        &endpoints,
        Arc::new(MemoryRepo::default()),
        "client".to_owned(),
        "secret".to_owned(),
    );
    manager
        .save_from_bundle(KickAuthBundle {
            access_token: "stale-access".to_owned(),
            refresh_token: "stale-refresh".to_owned(),
            user_id: 7,
            username: "streamer".to_owned(),
            client_id: "client".to_owned(),
            expires_at: OffsetDateTime::UNIX_EPOCH,
        })
        .await
        .unwrap();

    let token = manager.get_valid_access_token().await.unwrap();

    assert_eq!(token, "fresh-access");
}

async fn mount_channel(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v2/channels/streamer"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "chatroom": { "id": CHATROOM_ID },
            "livestream": null
        })))
        .mount(server)
        .await;
}

struct PathCapture<'a>(&'a mut String);

impl Callback for PathCapture<'_> {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        *self.0 = request.uri().path().to_owned();
        Ok(response)
    }
}

async fn spawn_pusher_peer() -> (String, oneshot::Receiver<(String, Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let socket_addr = listener.local_addr().unwrap();
    let (seen_tx, seen_rx) = oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut request_path = String::new();
        let mut ws = tokio_tungstenite::accept_hdr_async(stream, PathCapture(&mut request_path))
            .await
            .unwrap();
        while let Some(Ok(frame)) = ws.next().await {
            if let Message::Text(text) = frame {
                let parsed: Value = serde_json::from_str(&text).unwrap();
                seen_tx.send((request_path, parsed)).unwrap();
                break;
            }
        }
        while let Some(Ok(_)) = ws.next().await {}
    });
    (format!("ws://{socket_addr}/app"), seen_rx)
}

fn expected_handshake() -> (String, Value) {
    (
        format!("/app/{PUSHER_APP_KEY}"),
        json!({
            "event": "pusher:subscribe",
            "data": { "channel": format!("chatrooms.{CHATROOM_ID}.v2") }
        }),
    )
}

#[tokio::test]
async fn chat_socket_override_carries_the_pusher_handshake_and_subscribe_to_the_override_host() {
    let channel_api = MockServer::start().await;
    mount_channel(&channel_api).await;
    let (socket_base, seen_rx) = spawn_pusher_peer().await;
    let endpoints = endpoints(&[
        (
            EndpointSurface::KickChannelApi,
            format!("{}/api/v2", channel_api.uri()),
        ),
        (EndpointSurface::KickChatSocket, socket_base),
    ]);
    let (event_tx, _event_rx) = mpsc::channel(8);

    let handle = KickChat::new(&endpoints, "streamer".to_owned(), reqwest::Client::new())
        .connect(event_tx)
        .await
        .unwrap();
    let handshake = tokio::time::timeout(STEP_DEADLINE, seen_rx)
        .await
        .unwrap()
        .unwrap();
    handle.shutdown();

    assert_eq!(handshake, expected_handshake());
}

#[tokio::test]
async fn platform_carries_its_overrides_into_the_chat_session_and_the_sender() {
    let server = MockServer::start().await;
    mount_channel(&server).await;
    Mock::given(method("POST"))
        .and(path("/public/v1/chat"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    let (socket_base, seen_rx) = spawn_pusher_peer().await;
    let endpoints = endpoints(&[
        (
            EndpointSurface::KickPublicApi,
            format!("{}/public/v1", server.uri()),
        ),
        (
            EndpointSurface::KickChannelApi,
            format!("{}/api/v2", server.uri()),
        ),
        (EndpointSurface::KickChatSocket, socket_base),
    ]);
    let manager = KickCredentialsManager::new(
        &endpoints,
        Arc::new(MemoryRepo::default()),
        "client".to_owned(),
        "secret".to_owned(),
    );
    manager
        .save_from_bundle(KickAuthBundle {
            access_token: "live-access".to_owned(),
            refresh_token: "live-refresh".to_owned(),
            user_id: 7,
            username: "streamer".to_owned(),
            client_id: "client".to_owned(),
            expires_at: OffsetDateTime::now_utc() + time::Duration::hours(1),
        })
        .await
        .unwrap();
    let platform = KickPlatform::new(&endpoints, Arc::new(manager), limiter());

    platform.connect().await.unwrap();
    platform.send_message("streamer", "hello").await.unwrap();
    let handshake = tokio::time::timeout(STEP_DEADLINE, seen_rx)
        .await
        .unwrap()
        .unwrap();
    platform.disconnect().await.unwrap();

    assert_eq!(handshake, expected_handshake());
}
