#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use forge_emulator::EmulatorError;
use forge_emulator::fixture::KickAccount;
use forge_emulator::kick::{
    FakeKick, FakeKickConfig, FakeKickSetup, KickChatter, KickCredentialCheck, KickLedger,
    KickSurface, RefreshAnswer,
};
use forge_events::Event;
use forge_platform_core::{
    EndpointSurface, PlatformEndpoints, PlatformError, RateLimitOutcome, RateLimiter,
};
use forge_platform_kick::chat::PUSHER_APP_KEY;
use forge_platform_kick::error::KickError;
use forge_platform_kick::{
    ChannelInfoFetcher, KickAuthBundle, KickChannel, KickChat, KickChatHandle,
    KickCredentialsManager, KickRewards, KickSendChat,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const WAIT: Duration = Duration::from_secs(10);
const FOREIGN_SLUG: &str = "someone_else";

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
        Ok(Vec::new())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

async fn start(setup: FakeKickSetup) -> FakeKick {
    FakeKick::start(FakeKickConfig::for_account(&KickAccount::default(), &setup))
        .await
        .unwrap()
}

fn endpoints(fake: &FakeKick) -> PlatformEndpoints {
    let overrides = fake.endpoint_overrides();
    PlatformEndpoints::resolve(|variable| {
        overrides
            .iter()
            .find(|(name, _)| *name == variable)
            .map(|(_, url)| OsString::from(url))
    })
    .unwrap()
}

fn limiter() -> Arc<dyn RateLimiter> {
    Arc::new(GrantLimiter)
}

fn http_base(fake: &FakeKick) -> String {
    let public = &fake.endpoint_overrides()[0].1;
    public.trim_end_matches("/public/v1").to_owned()
}

async fn join_chat(fake: &FakeKick) -> (KickChatHandle, mpsc::Receiver<Event>) {
    let (event_tx, event_rx) = mpsc::channel(16);
    let handle = KickChat::new(
        &endpoints(fake),
        fake.config().username.clone(),
        reqwest::Client::new(),
    )
    .connect(event_tx)
    .await
    .unwrap();
    let channel = fake.chat_channel();
    fake.wait_for("forge to join the chatroom", WAIT, |ledger| {
        ledger.joined(&channel)
    })
    .await
    .unwrap();
    (handle, event_rx)
}

async fn manager(fake: &FakeKick, client_secret: &str) -> KickCredentialsManager {
    let account = KickAccount::default();
    let manager = KickCredentialsManager::new(
        &endpoints(fake),
        Arc::new(MemoryCredentials::default()),
        account.client_id.clone(),
        client_secret.to_owned(),
    );
    manager
        .save_from_bundle(KickAuthBundle {
            access_token: account.access_token,
            refresh_token: account.refresh_token,
            user_id: account.user_id,
            username: account.username,
            client_id: account.client_id,
            expires_at: OffsetDateTime::UNIX_EPOCH,
        })
        .await
        .unwrap();
    manager
}

async fn raw_socket(fake: &FakeKick, app_key: &str) -> Socket {
    let base = &fake.endpoint_overrides()[3].1;
    tokio_tungstenite::connect_async(format!("{base}/{app_key}?protocol=7"))
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

async fn send_frame(socket: &mut Socket, frame: Value) {
    socket
        .send(Message::Text(frame.to_string().into()))
        .await
        .unwrap();
}

fn last_request(ledger: &KickLedger) -> &forge_emulator::kick::KickRequest {
    ledger.requests.last().unwrap()
}

#[tokio::test]
async fn overrides_cover_every_kick_surface_and_pass_forges_own_validation() {
    let fake = start(FakeKickSetup::default()).await;
    let resolved = endpoints(&fake);
    let overridden: Vec<EndpointSurface> = resolved.overridden().map(|(s, _)| s).collect();
    assert_eq!(
        overridden,
        vec![
            EndpointSurface::KickPublicApi,
            EndpointSurface::KickChannelApi,
            EndpointSurface::KickChatSocket,
            EndpointSurface::KickOAuth,
        ]
    );
}

#[tokio::test]
async fn forge_chat_joins_the_chatroom_the_channel_api_names() {
    let fake = start(FakeKickSetup::default()).await;
    let (handle, _events) = join_chat(&fake).await;
    let ledger = fake.ledger();
    handle.shutdown();
    let lookup = &ledger.requests[0];
    assert_eq!(
        (
            lookup.surface,
            lookup.path.as_str(),
            lookup.status,
            ledger.sessions[0].app_key.as_str()
        ),
        (
            KickSurface::ChannelApi,
            "/api/v2/channels/forge_emulator",
            200,
            PUSHER_APP_KEY
        )
    );
}

#[tokio::test]
async fn injected_chat_message_reaches_forge_as_a_kick_chat_message() {
    let fake = start(FakeKickSetup::default()).await;
    let (handle, mut events) = join_chat(&fake).await;
    let chatter = KickChatter {
        user_id: 200_000_042,
        username: "alice_kick".to_owned(),
    };
    let message_id = fake
        .inject_chat_message(&chatter, "!hello there")
        .await
        .unwrap();
    let event = timeout(WAIT, events.recv()).await.unwrap().unwrap();
    handle.shutdown();
    assert_eq!(
        (
            event.kind.as_str(),
            &event.payload["message_id"],
            &event.payload["content"],
            &event.payload["sender"]["id"],
            &event.payload["sender"]["username"],
        ),
        (
            "kick.chat.message.sent",
            &json!(message_id),
            &json!("!hello there"),
            &json!(200_000_042),
            &json!("alice_kick"),
        )
    );
}

#[tokio::test]
async fn pushed_event_without_a_joined_connection_is_refused() {
    let fake = start(FakeKickSetup::default()).await;
    let refused = fake
        .push_event("App\\Events\\ChatMessageEvent", &json!({}))
        .await;
    assert!(
        matches!(&refused, Err(EmulatorError::KickChannelNotJoined { channel }) if *channel == fake.chat_channel()),
        "{refused:?}"
    );
}

#[tokio::test]
async fn closed_forge_connection_no_longer_counts_as_joined() {
    let fake = start(FakeKickSetup::default()).await;
    let (handle, _events) = join_chat(&fake).await;
    handle.shutdown();
    let channel = fake.chat_channel();
    let left = fake
        .wait_for("the connection to close", WAIT, |ledger| {
            ledger.joined(&channel).is_none().then_some(())
        })
        .await;
    assert!(left.is_ok(), "{:?}", fake.ledger().sessions);
}

#[tokio::test]
async fn foreign_app_key_is_answered_with_pusher_error_4001() {
    let fake = start(FakeKickSetup::default()).await;
    let mut socket = raw_socket(&fake, "not-the-kick-key").await;
    let refusal = next_frame(&mut socket).await.unwrap();
    let data: Value = serde_json::from_str(refusal["data"].as_str().unwrap()).unwrap();
    assert_eq!(
        (&refusal["event"], &data["code"]),
        (&json!("pusher:error"), &json!(4001))
    );
}

#[tokio::test]
async fn ping_is_answered_with_pong() {
    let fake = start(FakeKickSetup::default()).await;
    let mut socket = raw_socket(&fake, PUSHER_APP_KEY).await;
    let greeting = next_frame(&mut socket).await.unwrap();
    send_frame(&mut socket, json!({ "event": "pusher:ping", "data": {} })).await;
    let reply = next_frame(&mut socket).await.unwrap();
    assert_eq!(
        (&greeting["event"], &reply["event"]),
        (
            &json!("pusher:connection_established"),
            &json!("pusher:pong")
        )
    );
}

#[tokio::test]
async fn subscribe_is_acknowledged_on_the_subscribed_channel() {
    let fake = start(FakeKickSetup::default()).await;
    let channel = fake.chat_channel();
    let mut socket = raw_socket(&fake, PUSHER_APP_KEY).await;
    next_frame(&mut socket).await.unwrap();
    send_frame(
        &mut socket,
        json!({ "event": "pusher:subscribe", "data": { "channel": channel } }),
    )
    .await;
    let ack = next_frame(&mut socket).await.unwrap();
    assert_eq!(
        (&ack["event"], &ack["channel"]),
        (
            &json!("pusher_internal:subscription_succeeded"),
            &json!(channel)
        )
    );
}

#[tokio::test]
async fn unsubscribed_connection_stops_receiving_chatroom_events() {
    let fake = start(FakeKickSetup::default()).await;
    let channel = fake.chat_channel();
    let mut socket = raw_socket(&fake, PUSHER_APP_KEY).await;
    next_frame(&mut socket).await.unwrap();
    send_frame(
        &mut socket,
        json!({ "event": "pusher:subscribe", "data": { "channel": channel } }),
    )
    .await;
    next_frame(&mut socket).await.unwrap();
    send_frame(
        &mut socket,
        json!({ "event": "pusher:unsubscribe", "data": { "channel": channel } }),
    )
    .await;
    fake.wait_for("the unsubscribe", WAIT, |ledger| {
        ledger.joined(&channel).is_none().then_some(())
    })
    .await
    .unwrap();
    let pushed = fake
        .push_event("App\\Events\\ChatMessageEvent", &json!({}))
        .await;
    assert!(
        matches!(pushed, Err(EmulatorError::KickChannelNotJoined { .. })),
        "{pushed:?}"
    );
}

#[tokio::test]
async fn forge_send_posts_the_message_as_the_broadcaster() {
    let fake = start(FakeKickSetup::default()).await;
    let account = KickAccount::default();
    KickSendChat::new(&endpoints(&fake), limiter())
        .send("hi chat", &account.access_token, account.user_id, false)
        .await
        .unwrap();
    let ledger = fake.ledger();
    let request = last_request(&ledger);
    assert_eq!(
        (request.credentials, request.body.clone()),
        (
            KickCredentialCheck::Accepted,
            Some(
                json!({ "content": "hi chat", "type": "user", "broadcaster_user_id": account.user_id })
            )
        )
    );
}

#[tokio::test]
async fn public_api_without_the_current_token_is_unauthorized() {
    let fake = start(FakeKickSetup::default()).await;
    let url = format!("{}/public/v1/channels", http_base(&fake));
    for (authorization, expected) in [
        (None, KickCredentialCheck::MissingBearer),
        (Some("Bearer stale-token"), KickCredentialCheck::WrongBearer),
    ] {
        let mut request = reqwest::Client::new().get(&url);
        if let Some(value) = authorization {
            request = request.header("authorization", value);
        }
        let status = request.send().await.unwrap().status();
        let ledger = fake.ledger();
        assert_eq!(
            (status.as_u16(), last_request(&ledger).credentials),
            (401, expected),
            "{authorization:?}"
        );
    }
}

#[tokio::test]
async fn malformed_send_bodies_are_refused_with_400() {
    let fake = start(FakeKickSetup::default()).await;
    let account = KickAccount::default();
    let url = format!("{}/public/v1/chat", http_base(&fake));
    for body in [
        json!({ "content": "", "type": "bot" }),
        json!({ "content": "hi" }),
        json!({ "content": "hi", "type": "user" }),
        json!({ "content": "hi", "type": "user", "broadcaster_user_id": 1 }),
    ] {
        let status = reqwest::Client::new()
            .post(&url)
            .bearer_auth(&account.access_token)
            .json(&body)
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status.as_u16(), 400, "{body}");
    }
}

#[tokio::test]
async fn forge_delete_message_is_answered_with_no_content() {
    let fake = start(FakeKickSetup::default()).await;
    let deleted = KickSendChat::new(&endpoints(&fake), limiter())
        .delete("msg-1", &KickAccount::default().access_token)
        .await;
    assert!(deleted.is_ok(), "{deleted:?}");
}

#[tokio::test]
async fn forge_channel_read_follows_the_stream_state() {
    let fake = start(FakeKickSetup {
        stream_title: "marathon".to_owned(),
        viewer_count: 31,
        ..FakeKickSetup::default()
    })
    .await;
    let channel = KickChannel::new(&endpoints(&fake), limiter());
    let token = KickAccount::default().access_token;
    let offline = channel.get_channel(&token).await.unwrap();
    fake.set_live(true);
    let live = channel.get_channel(&token).await.unwrap();
    assert_eq!(
        (
            (offline.is_live, offline.viewer_count),
            (live.is_live, live.viewer_count, live.stream_title.as_str()),
        ),
        ((false, 0), (true, 31, "marathon"))
    );
}

#[tokio::test]
async fn channel_read_for_a_foreign_slug_finds_no_channel() {
    let fake = start(FakeKickSetup::default()).await;
    let read = KickChannel::new(&endpoints(&fake), limiter())
        .get_channel_by_slug(&KickAccount::default().access_token, FOREIGN_SLUG)
        .await;
    assert!(
        matches!(&read, Err(PlatformError::Http { body, .. }) if body.contains("no data")),
        "{:?}",
        read.as_ref().map(|snapshot| snapshot.slug.clone())
    );
}

#[tokio::test]
async fn channel_info_for_an_unknown_slug_is_not_found() {
    let fake = start(FakeKickSetup::default()).await;
    let fetched = ChannelInfoFetcher::new(
        &endpoints(&fake),
        FOREIGN_SLUG.to_owned(),
        reqwest::Client::new(),
    )
    .fetch()
    .await;
    assert!(
        matches!(fetched, Err(KickError::Http { status: 404, .. })),
        "{fetched:?}"
    );
}

#[tokio::test]
async fn forge_redemption_poll_finds_nothing_pending() {
    let fake = start(FakeKickSetup::default()).await;
    let pending = KickRewards::new(&endpoints(&fake), limiter())
        .list_pending_redemptions(&KickAccount::default().access_token)
        .await
        .unwrap();
    assert!(pending.is_empty());
}

#[tokio::test]
async fn accepted_refresh_hands_forge_the_token_the_fake_now_accepts() {
    let fake = start(FakeKickSetup::default()).await;
    let manager = manager(&fake, &KickAccount::default().client_secret).await;
    let token = manager.get_valid_access_token().await.unwrap();
    assert_eq!(
        (token.as_str() != KickAccount::default().access_token, token),
        (true, fake.access_token())
    );
}

#[tokio::test]
async fn token_retired_by_a_refresh_is_unauthorized() {
    let fake = start(FakeKickSetup::default()).await;
    let manager = manager(&fake, &KickAccount::default().client_secret).await;
    manager.get_valid_access_token().await.unwrap();
    let account = KickAccount::default();
    let sent = KickSendChat::new(&endpoints(&fake), limiter())
        .send("hi", &account.access_token, account.user_id, false)
        .await;
    assert!(matches!(sent, Err(PlatformError::Auth { .. })), "{sent:?}");
}

#[tokio::test]
async fn refused_refresh_requires_reauthentication() {
    let account = KickAccount::default();
    for (refresh, client_secret) in [
        (RefreshAnswer::Reject, account.client_secret.as_str()),
        (RefreshAnswer::Accept, "not-the-client-secret"),
    ] {
        let fake = start(FakeKickSetup {
            refresh,
            ..FakeKickSetup::default()
        })
        .await;
        let manager = manager(&fake, client_secret).await;
        let refreshed = manager.get_valid_access_token().await;
        let ledger = fake.ledger();
        assert!(
            matches!(refreshed, Err(PlatformError::ReauthRequired { .. })),
            "{refresh:?}: {refreshed:?}"
        );
        assert_eq!(
            (last_request(&ledger).surface, last_request(&ledger).status),
            (KickSurface::OAuth, 400),
            "{refresh:?}"
        );
    }
}

#[tokio::test]
async fn refresh_token_spent_by_a_rotation_is_refused() {
    let fake = start(FakeKickSetup::default()).await;
    let account = KickAccount::default();
    manager(&fake, &account.client_secret)
        .await
        .get_valid_access_token()
        .await
        .unwrap();
    let replayed = manager(&fake, &account.client_secret)
        .await
        .get_valid_access_token()
        .await;
    assert!(
        matches!(replayed, Err(PlatformError::ReauthRequired { .. })),
        "{replayed:?}"
    );
}

#[tokio::test]
async fn unmodeled_routes_are_recorded_as_unexpected_404s() {
    let fake = start(FakeKickSetup::default()).await;
    let token = KickAccount::default().access_token;
    let client = reqwest::Client::new();
    let base = http_base(&fake);
    for (method, path, surface) in [
        ("GET", "/public/v1/users", KickSurface::PublicApi),
        ("POST", "/public/v1/moderation/bans", KickSurface::PublicApi),
        (
            "GET",
            "/api/v2/channels/forge_emulator/messages",
            KickSurface::ChannelApi,
        ),
        ("GET", "/oauth/token", KickSurface::OAuth),
        ("GET", "/elsewhere", KickSurface::Unknown),
    ] {
        let status = client
            .request(method.parse().unwrap(), format!("{base}{path}"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status();
        let ledger = fake.ledger();
        let recorded = last_request(&ledger);
        assert_eq!(
            (status.as_u16(), recorded.modeled, recorded.surface),
            (404, false, surface),
            "{method} {path}"
        );
    }
}

#[test]
fn debug_output_never_carries_kick_secrets() {
    let account = KickAccount::default();
    let config = FakeKickConfig::default();
    let rendered = format!("{account:?} {config:?}");
    for secret in [
        &account.access_token,
        &account.refresh_token,
        &account.client_secret,
    ] {
        assert!(!rendered.contains(secret.as_str()), "{rendered}");
    }
}
