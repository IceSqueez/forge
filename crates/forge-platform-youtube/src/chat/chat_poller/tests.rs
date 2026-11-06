use std::time::Duration;

use futures::future::BoxFuture;

use super::*;
use crate::api::quota_state::BROADCAST_COST;
use crate::endpoints_test_support::{
    CapturingPublisher, StubReply, StubServer, data_api_with, used_today, wait_until,
};

const REQUEST_FAIL_KIND: &str = "request.fail";
use crate::api::quota_state::QuotaState;
use crate::api::quota_state::SharedQuota;
use crate::broadcast::live_chat_id::LiveChatIdHandle;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn token_source() -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>
{
    Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
}

fn broadcast_response(live_chat_id: &str) -> serde_json::Value {
    json!({
        "items": [{
            "id": "broadcast-1",
            "snippet": {
                "liveChatId": live_chat_id,
                "title": "Test Stream"
            }
        }]
    })
}

fn empty_broadcast_response() -> serde_json::Value {
    json!({ "items": [] })
}

fn chat_response(items: serde_json::Value, polling_ms: u64) -> serde_json::Value {
    json!({
        "pollingIntervalMillis": polling_ms,
        "items": items
    })
}

fn chat_response_with_page(
    items: serde_json::Value,
    polling_ms: u64,
    next_page_token: &str,
) -> serde_json::Value {
    json!({
        "pollingIntervalMillis": polling_ms,
        "nextPageToken": next_page_token,
        "items": items
    })
}

fn text_item(id: &str, text: &str, display_name: &str) -> serde_json::Value {
    json!({
        "id": id,
        "snippet": {
            "type": "textMessageEvent",
            "displayMessage": text
        },
        "authorDetails": {
            "displayName": display_name,
            "channelId": "UCviewer",
            "isChatOwner": false,
            "isChatModerator": false,
            "isChatSponsor": false
        }
    })
}

async fn mount_broadcast_mock(server: &MockServer, response: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path("/liveBroadcasts"))
        .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(response))
        .mount(server)
        .await;
}

async fn mount_chat_mock(server: &MockServer, response: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path("/liveChat/messages"))
        .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(response))
        .mount(server)
        .await;
}

fn make_quota() -> SharedQuota {
    SharedQuota::default()
}

fn make_poller(server: &MockServer) -> (YouTubeChatPoller, UnboundedSender<Event>) {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_at(&server.uri()),
        token_source(),
        tx.clone(),
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    );
    (poller, tx)
}

fn make_poller_with_receiver(
    server: &MockServer,
) -> (
    YouTubeChatPoller,
    tokio::sync::mpsc::UnboundedReceiver<Event>,
) {
    make_poller_with_ledger(
        server,
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    )
}

fn make_poller_with_ledger(
    server: &MockServer,
    ban_ledger: Arc<dyn BanLedgerRepo>,
) -> (
    YouTubeChatPoller,
    tokio::sync::mpsc::UnboundedReceiver<Event>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_at(&server.uri()),
        token_source(),
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        ban_ledger,
    );
    (poller, rx)
}

async fn assert_leading_online(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Event>) {
    let online = tokio::time::timeout(Duration::from_secs(30), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(online.kind, "youtube.stream.online");
}

const FAST: ChatPollTimings = ChatPollTimings {
    poll_floor: Duration::from_millis(1),
    long_interval: Duration::from_millis(1),
    broadcast_cadence: Duration::from_millis(25),
};
const BROADCASTS: &str = "/liveBroadcasts";
const CHAT_MESSAGES: &str = "/liveChat/messages";

fn resolution_probe(server: &StubServer) -> (TokenSource, Arc<std::sync::atomic::AtomicUsize>) {
    let resolved = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&resolved);
    let broadcasts_answered = server.request_counter(BROADCASTS);
    let source: TokenSource = Arc::new(move || {
        seen.fetch_max(broadcasts_answered(), std::sync::atomic::Ordering::SeqCst);
        Box::pin(async { Ok("test-token".to_owned()) })
    });
    (source, resolved)
}

fn stub_poller_with_quota(
    server: &StubServer,
    quota: SharedQuota,
) -> (
    YouTubeChatPoller,
    Arc<std::sync::atomic::AtomicUsize>,
    tokio::sync::mpsc::UnboundedReceiver<Event>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let (source, resolved) = resolution_probe(server);
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_charging(&server.uri(), quota),
        source,
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    )
    .with_timings(FAST);
    (poller, resolved, rx)
}

async fn run_through_resolutions(
    poller: YouTubeChatPoller,
    resolved: &std::sync::atomic::AtomicUsize,
    resolutions: usize,
) {
    let cancel = CancellationToken::new();
    let join = tokio::spawn(poller.run(cancel.clone()));

    let reached =
        wait_until(|| resolved.load(std::sync::atomic::Ordering::SeqCst) >= resolutions).await;

    cancel.cancel();
    join.await.unwrap().unwrap();
    assert!(
        reached,
        "poller finished {} of the {resolutions} broadcast resolutions",
        resolved.load(std::sync::atomic::Ordering::SeqCst),
    );
}

async fn drain_events_over_broadcast_cycles(
    poller: YouTubeChatPoller,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
    resolved: &std::sync::atomic::AtomicUsize,
    resolutions: usize,
) -> Vec<Event> {
    run_through_resolutions(poller, resolved, resolutions).await;

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    events
}

#[tokio::test]
async fn text_message_event_emits_youtube_chat_message() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-abc")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([text_item("msg-1", "hello world", "StreamFan")]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.chat.message");
    assert_eq!(event.source, EventSource::YouTube);

    let chat: ChatPayload =
        serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
    assert_eq!(chat.author, "StreamFan");
    assert_eq!(chat.platform_msg_id, "msg-1");
    assert!(!chat.is_event);
    assert_eq!(
        chat.segments,
        vec![ChatSegment::Text {
            text: "hello world".to_owned()
        }]
    );
}

#[tokio::test]
async fn chat_message_payload_carries_author_entity_from_author_details() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-author")).await;
    mount_chat_mock(
        &server,
        chat_response(json!([text_item("msg-a", "hi", "StreamFan")]), 3000),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(
        event.payload["author"]["display_name"].as_str().unwrap(),
        "StreamFan"
    );
    assert_eq!(
        event.payload["author"]["channel_id"].as_str().unwrap(),
        "UCviewer"
    );
}

#[tokio::test]
async fn a_bang_text_is_published_once_as_a_plain_chat_message() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-cmd")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([
                text_item("cmd-1", "!shoutout user123", "Chatter"),
                text_item("msg-2", "after the command", "Chatter"),
            ]),
            3000,
        ),
    )
    .await;
    let (poller, mut rx) = make_poller_with_receiver(&server);
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move { poller.run(cancel_clone).await });
    assert_leading_online(&mut rx).await;

    let mut published = Vec::new();
    for _ in 0..2 {
        let event = tokio::time::timeout(Duration::from_secs(30), rx.recv())
            .await
            .unwrap()
            .unwrap();
        published.push((event.kind, event.payload["message_id"].clone()));
    }
    cancel.cancel();
    handle.await.unwrap().unwrap();

    assert_eq!(
        published,
        vec![
            ("youtube.chat.message".to_owned(), json!("cmd-1")),
            ("youtube.chat.message".to_owned(), json!("msg-2")),
        ]
    );
}

#[tokio::test]
async fn super_chat_event_emits_with_event_detail_super_chat() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-sc")).await;

    let item = json!({
        "id": "sc-1",
        "snippet": {
            "type": "superChatEvent",
            "superChatDetails": {
                "amountMicros": "5000000",
                "currency": "USD",
                "userComment": "great stream!"
            }
        },
        "authorDetails": {
            "displayName": "SuperFan",
            "isChatOwner": false,
            "isChatModerator": false,
            "isChatSponsor": false
        }
    });
    mount_chat_mock(&server, chat_response(json!([item]), 3000)).await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.chat.super_chat");

    let chat: ChatPayload =
        serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
    assert!(chat.is_event);
    assert_eq!(chat.author, "SuperFan");

    match chat.event_detail.unwrap() {
        ChatEventDetail::SuperChat { amount, message } => {
            assert_eq!(amount.micros(), 5_000_000);
            assert_eq!(amount.currency().as_str(), "USD");
            assert_eq!(message, Some("great stream!".to_owned()));
        }
        other => panic!("expected SuperChat, got {other:?}"),
    }
}

#[tokio::test]
async fn a_super_chat_with_an_unusable_currency_is_still_delivered_as_a_chat_line() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-sc")).await;

    let item = json!({
        "id": "sc-bad",
        "snippet": {
            "type": "superChatEvent",
            "displayMessage": "thanks for the stream",
            "superChatDetails": {
                "amountMicros": "5000000",
                "currency": "??",
                "userComment": "thanks for the stream"
            }
        },
        "authorDetails": {
            "displayName": "SuperFan",
            "isChatOwner": false,
            "isChatModerator": false,
            "isChatSponsor": false
        }
    });
    mount_chat_mock(&server, chat_response(json!([item]), 3000)).await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.chat.message");
    assert_eq!(event.payload["message_text"], "thanks for the stream");
}

fn banned_item(
    id: &str,
    display_name: &str,
    ban_type: &str,
    duration_secs: serde_json::Value,
) -> serde_json::Value {
    json!({
        "id": id,
        "snippet": {
            "type": "userBannedEvent",
            "userBannedDetails": {
                "bannedUserDetails": {
                    "displayName": display_name,
                    "channelId": "UCbanned"
                },
                "banType": ban_type,
                "banDurationSeconds": duration_secs
            }
        },
        "authorDetails": {
            "displayName": "ModAlice",
            "channelId": "UCmodalice"
        }
    })
}

#[tokio::test]
async fn user_banned_with_duration_emits_temporary_ban() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-ban-temp")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([banned_item("ban-1", "Troll", "temporary", json!(600))]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.channel.user_banned");
    assert_eq!(event.payload["type"].as_str().unwrap(), "temporary");
    assert_eq!(event.payload["duration_seconds"].as_i64().unwrap(), 600);
    assert_eq!(
        event.payload["target_user"]["display_name"]
            .as_str()
            .unwrap(),
        "Troll"
    );
}

#[tokio::test]
async fn permanent_ban_carries_null_duration_not_zero_placeholder() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-ban-perm")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([banned_item("ban-2", "Spammer", "permanent", json!(null))]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.channel.user_banned");
    assert_eq!(event.payload["type"].as_str().unwrap(), "permanent");
    assert!(event.payload["duration_seconds"].is_null());
}

#[tokio::test]
async fn user_banned_populates_moderator_from_author_details() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-ban-mod")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([banned_item("ban-3", "Troll", "permanent", json!(null))]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(
        event.payload["moderator"]["channel_id"].as_str().unwrap(),
        "UCmodalice"
    );
    assert_eq!(
        event.payload["moderator"]["display_name"].as_str().unwrap(),
        "ModAlice"
    );
}

#[test]
fn polling_interval_millis_respected() {
    let s = sleep_duration(5000, POLL_FLOOR_MS);
    assert_eq!(
        s,
        Duration::from_millis(5000),
        "pollingIntervalMillis > floor must be honored"
    );
}

#[test]
fn polling_interval_floor_3s() {
    let s = sleep_duration(1000, POLL_FLOOR_MS);
    assert_eq!(
        s,
        Duration::from_millis(POLL_FLOOR_MS),
        "pollingIntervalMillis below floor must use the 3s floor"
    );
}

#[tokio::test]
async fn dedup_via_next_page_token() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/liveBroadcasts"))
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::OK)
                .set_body_json(broadcast_response("chat-dedup")),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/liveChat/messages"))
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(chat_response_with_page(
                json!([
                    text_item("id1", "hello", "User1"),
                    text_item("id2", "world", "User2")
                ]),
                0,
                "ptX",
            )),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/liveChat/messages"))
        .and(query_param("pageToken", "ptX"))
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(chat_response(
                json!([
                    text_item("id2", "world", "User2"),
                    text_item("id3", "again", "User3")
                ]),
                0,
            )),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/liveChat/messages"))
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::OK)
                .set_body_json(chat_response(json!([]), 0)),
        )
        .mount(&server)
        .await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_at(&server.uri()),
        token_source(),
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    );

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

    assert_leading_online(&mut rx).await;

    let mut events = Vec::new();
    let timeout_result = tokio::time::timeout(Duration::from_secs(10), async {
        while events.len() < 3 {
            if let Some(e) = rx.recv().await {
                events.push(e);
            } else {
                break;
            }
        }
    })
    .await;

    cancel.cancel();
    handle.await.unwrap().unwrap();

    assert!(
        timeout_result.is_ok(),
        "timed out waiting for 3 unique events"
    );

    let ids: Vec<String> = events
        .iter()
        .filter_map(|e| {
            e.payload
                .get(ChatPayload::KEY)
                .and_then(|c| c.get("platform_msg_id"))
                .and_then(|v| v.as_str())
                .map(ToOwned::to_owned)
        })
        .collect();

    assert_eq!(
        ids.len(),
        3,
        "expected exactly 3 unique events, got: {ids:?}"
    );
    assert!(ids.contains(&"id1".to_owned()));
    assert!(ids.contains(&"id2".to_owned()));
    assert!(ids.contains(&"id3".to_owned()));
}

#[tokio::test]
async fn cancel_token_breaks_loop() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-cancel")).await;
    mount_chat_mock(&server, chat_response(json!([]), 3000)).await;

    let (poller, _rx) = make_poller(&server);
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

    tokio::time::timeout(Duration::from_secs(30), async {
        while server.received_requests().await.unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the poller never reached the API");
    cancel.cancel();

    let result = tokio::time::timeout(Duration::from_secs(30), handle)
        .await
        .expect("run() did not return within timeout after cancel")
        .unwrap();

    assert!(result.is_ok(), "run() must return Ok(()) on cancellation");
}

fn make_poller_with_both_handles(
    server: &MockServer,
) -> (
    YouTubeChatPoller,
    LiveChatIdHandle,
    ActiveBroadcastIdHandle,
    tokio::sync::mpsc::UnboundedReceiver<Event>,
) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let live = LiveChatIdHandle::new();
    let broadcast = ActiveBroadcastIdHandle::new();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_at(&server.uri()),
        token_source(),
        tx,
        "UCtest".to_owned(),
        live.clone(),
        broadcast.clone(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    );
    (poller, live, broadcast, rx)
}

#[tokio::test]
async fn active_broadcast_sets_broadcast_id_to_items_first_id_alongside_live_chat_id() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("lc-active-xyz")).await;
    mount_chat_mock(&server, chat_response(json!([]), 3000)).await;

    let (poller, live, broadcast, _rx) = make_poller_with_both_handles(&server);
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let join = tokio::spawn(async move { poller.run(cancel_clone).await });

    forge_test_support::settle_until("both handles were set", || {
        live.get().is_some() && broadcast.get().is_some()
    })
    .await;

    cancel.cancel();
    join.await.unwrap().unwrap();

    assert_eq!(live.get().as_deref(), Some("lc-active-xyz"));
    assert_eq!(broadcast.get().as_deref(), Some("broadcast-1"));
}

#[tokio::test]
async fn absent_broadcast_clears_both_handles_and_emits_nothing_when_never_live() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, empty_broadcast_response()).await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let live = LiveChatIdHandle::new();
    live.set(Some("stale-lc".to_owned()));
    let broadcast = ActiveBroadcastIdHandle::new();
    broadcast.set(Some("stale-bc".to_owned()));

    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_at(&server.uri()),
        token_source(),
        tx,
        "UCtest".to_owned(),
        live.clone(),
        broadcast.clone(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    );

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let join = tokio::spawn(async move { poller.run(cancel_clone).await });

    forge_test_support::settle_until("both handles were cleared", || {
        live.get().is_none() && broadcast.get().is_none()
    })
    .await;

    cancel.cancel();
    join.await.unwrap().unwrap();

    assert!(
        rx.try_recv().is_err(),
        "a poller that was never live must emit no lifecycle event"
    );
}

fn tombstone_item(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "snippet": { "type": "tombstone" }
    })
}

fn legacy_message_deleted_item(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "snippet": {
            "type": "messageDeletedEvent",
            "messageDeletedDetails": {
                "deletedMessageId": "removed-msg"
            }
        },
        "authorDetails": {
            "displayName": "ModName",
            "channelId": "UCmod",
            "isChatOwner": false,
            "isChatModerator": true,
            "isChatSponsor": false
        }
    })
}

fn membership_gifting_item(
    id: &str,
    count: serde_json::Value,
    level_name: &str,
    gifter_display_name: &str,
    gifter_channel_id: &str,
) -> serde_json::Value {
    json!({
        "id": id,
        "snippet": {
            "type": "membershipGiftingEvent",
            "membershipGiftingDetails": {
                "giftMembershipsCount": count,
                "giftMembershipsLevelName": level_name
            }
        },
        "authorDetails": {
            "displayName": gifter_display_name,
            "channelId": gifter_channel_id,
            "isChatOwner": false,
            "isChatModerator": false,
            "isChatSponsor": true
        }
    })
}

fn gift_received_item(
    id: &str,
    level_name: &str,
    recipient_display_name: &str,
    recipient_channel_id: &str,
    gifter_channel_id: serde_json::Value,
) -> serde_json::Value {
    json!({
        "id": id,
        "snippet": {
            "type": "giftMembershipReceivedEvent",
            "giftMembershipReceivedDetails": {
                "memberLevelName": level_name,
                "gifterChannelId": gifter_channel_id
            }
        },
        "authorDetails": {
            "displayName": recipient_display_name,
            "channelId": recipient_channel_id,
            "isChatOwner": false,
            "isChatModerator": false,
            "isChatSponsor": true
        }
    })
}

async fn first_event_from(server: &MockServer) -> Event {
    let (poller, mut rx) = make_poller_with_receiver(server);
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move { poller.run(cancel_clone).await });

    assert_leading_online(&mut rx).await;
    let event = tokio::time::timeout(Duration::from_secs(30), rx.recv())
        .await
        .unwrap()
        .unwrap();

    cancel.cancel();
    handle.await.unwrap().unwrap();
    event
}

#[tokio::test]
async fn tombstone_emits_message_deleted_with_message_id_and_deleted_mark() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-del")).await;
    mount_chat_mock(
        &server,
        chat_response(json!([tombstone_item("del-1")]), 3000),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.chat.message_deleted");
    assert_eq!(event.payload["message_id"].as_str().unwrap(), "del-1");

    let chat: ChatPayload =
        serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
    assert!(chat.moderation.deleted);
    assert!(!chat.moderation.banned);
    assert!(!chat.moderation.timed_out);

    assert_eq!(
        moderation_action(&event),
        ChatModerationAction::DeleteMessage {
            message_id: "del-1".to_owned()
        }
    );
}

fn moderation_action(event: &Event) -> ChatModerationAction {
    let envelope = event.payload[ChatModerationPayload::KEY].clone();
    serde_json::from_value::<ChatModerationPayload>(envelope)
        .unwrap()
        .action
}

#[tokio::test]
async fn a_temporary_ban_names_the_channel_to_time_out_and_a_permanent_one_to_ban() {
    for (item, removal) in [
        (
            banned_item("ban-t", "Troll", "temporary", json!(600)),
            UserRemoval::Timeout,
        ),
        (
            banned_item("ban-p", "Troll", "permanent", json!(null)),
            UserRemoval::Ban,
        ),
    ] {
        let server = MockServer::start().await;
        mount_broadcast_mock(&server, broadcast_response("chat-ban-mod")).await;
        mount_chat_mock(&server, chat_response(json!([item]), 3000)).await;

        let event = first_event_from(&server).await;

        assert_eq!(
            moderation_action(&event),
            ChatModerationAction::RemoveUser {
                user_id: Some("UCbanned".to_owned()),
                user_name: "Troll".to_owned(),
                removal,
            }
        );
    }
}

#[tokio::test]
async fn legacy_message_deleted_wire_type_produces_no_event() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-del-legacy")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([
                legacy_message_deleted_item("ignored-1"),
                text_item("keep-1", "still here", "Bystander")
            ]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.chat.message");
    let chat: ChatPayload =
        serde_json::from_value(event.payload[ChatPayload::KEY].clone()).unwrap();
    assert_eq!(chat.platform_msg_id, "keep-1");
}

#[tokio::test]
async fn membership_gifting_event_emits_count_level_and_gifter() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-gift")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([membership_gifting_item(
                "gift-1",
                json!(5),
                "Diamond",
                "Generous",
                "UCgifter"
            )]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.channel.member_gift");
    assert_eq!(event.payload["count"].as_i64().unwrap(), 5);
    assert_eq!(event.payload["level_name"].as_str().unwrap(), "Diamond");
    assert_eq!(
        event.payload["gifter"]["channel_id"].as_str().unwrap(),
        "UCgifter"
    );
    assert_eq!(
        event.payload["gifter"]["display_name"].as_str().unwrap(),
        "Generous"
    );
}

#[tokio::test]
async fn membership_gifting_event_parses_string_count_into_int() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-gift-str")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([membership_gifting_item(
                "gift-2",
                json!("12"),
                "Gold",
                "Patron",
                "UCpatron"
            )]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(event.payload["count"].as_i64().unwrap(), 12);
}

#[tokio::test]
async fn gift_membership_received_yields_null_gifter_when_wire_absent() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-gift-recv")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([gift_received_item(
                "recv-1",
                "Gold",
                "LuckyViewer",
                "UCrecipient",
                json!(null)
            )]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(event.kind, "youtube.channel.member_gift_received");
    assert_eq!(event.payload["level_name"].as_str().unwrap(), "Gold");
    assert_eq!(
        event.payload["recipient"]["channel_id"].as_str().unwrap(),
        "UCrecipient"
    );
    assert_eq!(
        event.payload["recipient"]["display_name"].as_str().unwrap(),
        "LuckyViewer"
    );
    assert!(event.payload["gifter"]["channel_id"].is_null());
    assert!(event.payload["gifter"]["display_name"].is_null());
}

#[tokio::test]
async fn gift_membership_received_reads_wire_gifter_channel_id() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-gift-recv-src")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([gift_received_item(
                "recv-2",
                "Gold",
                "LuckyViewer",
                "UCrecipient",
                json!("UCbenefactor")
            )]),
            3000,
        ),
    )
    .await;

    let event = first_event_from(&server).await;

    assert_eq!(
        event.payload["gifter"]["channel_id"].as_str().unwrap(),
        "UCbenefactor"
    );
}

fn broadcast_response_titled(live_chat_id: &str, title: &str) -> serde_json::Value {
    json!({
        "items": [{
            "id": "broadcast-1",
            "snippet": {
                "liveChatId": live_chat_id,
                "title": title
            }
        }]
    })
}

fn quota_for_two_resolutions() -> SharedQuota {
    SharedQuota::new(QuotaState {
        used_today: QUOTA_DAILY_LIMIT_FOR_TEST - 2 * BROADCAST_COST,
        last_reset_date: today_pacific(),
    })
}

const QUOTA_DAILY_LIMIT_FOR_TEST: u32 = 10_000;

fn reply_sequenced_broadcasts(
    server: &StubServer,
    first: &serde_json::Value,
    second: &serde_json::Value,
) {
    server.reply_times(BROADCASTS, StubReply::ok(first), 1);
    server.reply_times(BROADCASTS, StubReply::ok(second), 1);
    server.reply_always(BROADCASTS, StubReply::ok(second));
}

async fn run_until_resolution_and_drain_titles(
    poller: YouTubeChatPoller,
    resolved: &std::sync::atomic::AtomicUsize,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Event>,
    resolutions: usize,
) -> Vec<Event> {
    run_through_resolutions(poller, resolved, resolutions).await;

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        if e.kind == "youtube.stream.title_changed" {
            events.push(e);
        }
    }
    events
}

#[tokio::test]
async fn first_broadcast_resolution_records_title_without_emitting_event() {
    let server = StubServer::start().await;
    reply_sequenced_broadcasts(
        &server,
        &broadcast_response_titled("lc-1", "Steady Title"),
        &broadcast_response_titled("lc-2", "Steady Title"),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, quota_for_two_resolutions());
    let events = run_until_resolution_and_drain_titles(poller, &resolved, rx, 2).await;

    assert!(
        events.is_empty(),
        "first resolution must only record; unchanged title must not fire, got {} event(s)",
        events.len()
    );
}

#[tokio::test]
async fn changed_title_on_second_resolution_emits_one_title_changed_event() {
    let server = StubServer::start().await;
    reply_sequenced_broadcasts(
        &server,
        &broadcast_response_titled("lc-1", "Old Title"),
        &broadcast_response_titled("lc-2", "New Title"),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, quota_for_two_resolutions());
    let events = run_until_resolution_and_drain_titles(poller, &resolved, rx, 2).await;

    assert_eq!(
        events.len(),
        1,
        "a single title change must fire exactly one event"
    );
    let payload = &events[0].payload;
    assert_eq!(events[0].source, EventSource::YouTube);
    assert_eq!(payload["title"]["old"].as_str().unwrap(), "Old Title");
    assert_eq!(payload["title"]["new"].as_str().unwrap(), "New Title");
}

#[tokio::test]
async fn unchanged_title_across_two_resolutions_emits_no_event() {
    let server = StubServer::start().await;
    reply_sequenced_broadcasts(
        &server,
        &broadcast_response_titled("lc-1", "Same Title"),
        &broadcast_response_titled("lc-2", "Same Title"),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, quota_for_two_resolutions());
    let events = run_until_resolution_and_drain_titles(poller, &resolved, rx, 2).await;

    assert!(
        events.is_empty(),
        "identical titles must not fire, got {} event(s)",
        events.len()
    );
}

#[tokio::test]
async fn title_diff_across_intervening_offline_clear_does_not_fire() {
    let server = StubServer::start().await;
    server.reply_times(
        BROADCASTS,
        StubReply::ok(&broadcast_response_titled("lc-1", "Title A")),
        1,
    );
    server.reply_times(BROADCASTS, StubReply::ok(&empty_broadcast_response()), 1);
    server.reply_always(
        BROADCASTS,
        StubReply::ok(&broadcast_response_titled("lc-2", "Title B")),
    );

    let quota = SharedQuota::new(QuotaState {
        used_today: QUOTA_DAILY_LIMIT_FOR_TEST - 3 * BROADCAST_COST,
        last_reset_date: today_pacific(),
    });

    let (poller, resolved, rx) = stub_poller_with_quota(&server, quota);
    let events = run_until_resolution_and_drain_titles(poller, &resolved, rx, 3).await;

    assert!(
        events.is_empty(),
        "an offline clear between two live titles must reset last-seen; \
             go-live with a new title must not fire, got {} event(s)",
        events.len()
    );
}

fn chat_ended_item(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "snippet": { "type": "chatEndedEvent" }
    })
}

#[tokio::test]
async fn repeated_offline_resolutions_emit_no_events() {
    let server = StubServer::start().await;
    server.reply_always(BROADCASTS, StubReply::ok(&empty_broadcast_response()));

    let (poller, resolved, rx) = stub_poller_with_quota(&server, make_quota());
    let events = drain_events_over_broadcast_cycles(poller, rx, &resolved, 3).await;

    assert!(
        events.is_empty(),
        "repeated offline resolutions must stay silent, got {events:?}"
    );
}

#[tokio::test]
async fn going_live_emits_single_online_with_title_and_broadcast_id() {
    let server = StubServer::start().await;
    server.reply_always(BROADCASTS, StubReply::ok(&broadcast_response("lc-live")));
    server.reply_always(
        CHAT_MESSAGES,
        StubReply::ok(&chat_response(json!([]), 3000)),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, make_quota());
    let events = drain_events_over_broadcast_cycles(poller, rx, &resolved, 1).await;

    let online: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == "youtube.stream.online")
        .collect();
    assert_eq!(online.len(), 1, "exactly one online, got {events:?}");
    let payload = &online[0].payload;
    assert_eq!(
        payload[stream_fields::BROADCAST_TITLE].as_str().unwrap(),
        "Test Stream"
    );
    assert_eq!(
        payload[stream_fields::BROADCAST_ID].as_str().unwrap(),
        "broadcast-1"
    );
}

#[tokio::test]
async fn going_offline_emits_single_offline_with_last_broadcast_id() {
    let server = StubServer::start().await;
    reply_sequenced_broadcasts(
        &server,
        &broadcast_response("lc-live"),
        &empty_broadcast_response(),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, quota_for_two_resolutions());
    let events = drain_events_over_broadcast_cycles(poller, rx, &resolved, 2).await;

    let offline: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == "youtube.stream.offline")
        .collect();
    assert_eq!(offline.len(), 1, "exactly one offline, got {events:?}");
    assert_eq!(
        offline[0].payload[stream_fields::BROADCAST_ID]
            .as_str()
            .unwrap(),
        "broadcast-1"
    );
}

#[tokio::test]
async fn chat_ended_then_absent_broadcast_emits_single_offline_total() {
    let server = StubServer::start().await;
    reply_sequenced_broadcasts(
        &server,
        &broadcast_response("lc-live"),
        &empty_broadcast_response(),
    );
    server.reply_always(
        CHAT_MESSAGES,
        StubReply::ok(&chat_response(json!([chat_ended_item("ended-1")]), 0)),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, make_quota());
    let events = drain_events_over_broadcast_cycles(poller, rx, &resolved, 2).await;

    let offline = events
        .iter()
        .filter(|e| e.kind == "youtube.stream.offline")
        .count();
    assert_eq!(
        offline, 1,
        "chatEndedEvent then absent broadcast must not double-emit offline, got {events:?}"
    );
}

#[tokio::test]
async fn chat_ended_offline_carries_the_attached_broadcast_id_not_the_message_id() {
    let server = StubServer::start().await;
    server.reply_always(BROADCASTS, StubReply::ok(&broadcast_response("lc-live")));
    server.reply_always(
        CHAT_MESSAGES,
        StubReply::ok(&chat_response(json!([chat_ended_item("ended-msg-id")]), 0)),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, make_quota());
    let events = drain_events_over_broadcast_cycles(poller, rx, &resolved, 2).await;

    let offline: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == "youtube.stream.offline")
        .collect();
    assert_eq!(offline.len(), 1, "exactly one offline, got {events:?}");
    assert_eq!(
        offline[0].payload[stream_fields::BROADCAST_ID]
            .as_str()
            .unwrap(),
        "broadcast-1"
    );
}

#[tokio::test]
async fn duplicate_chat_ended_message_yields_exactly_one_offline() {
    let server = StubServer::start().await;
    server.reply_always(BROADCASTS, StubReply::ok(&broadcast_response("lc-live")));
    server.reply_always(
        CHAT_MESSAGES,
        StubReply::ok(&chat_response(
            json!([chat_ended_item("ended-1"), chat_ended_item("ended-1")]),
            0,
        )),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, make_quota());
    let events = drain_events_over_broadcast_cycles(poller, rx, &resolved, 2).await;

    let offline = events
        .iter()
        .filter(|e| e.kind == "youtube.stream.offline")
        .count();
    assert_eq!(offline, 1, "dedup must collapse repeats, got {events:?}");
}

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

fn counting_token_source() -> (TokenSource, Arc<std::sync::atomic::AtomicUsize>) {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let source: TokenSource = Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async { Ok("test-token".to_owned()) })
    });
    (source, calls)
}

#[tokio::test]
async fn ended_chat_is_not_repolled_and_next_discovery_waits_for_the_cadence() {
    let cadence = Duration::from_millis(200);
    let ended_responses = [
        (
            "chatEndedEvent",
            StubReply::ok(&chat_response(json!([chat_ended_item("ended-1")]), 0)),
        ),
        ("403 liveChatEnded", live_chat_ended()),
    ];

    for (label, response) in ended_responses {
        let server = StubServer::start().await;
        server.reply_always(BROADCASTS, StubReply::ok(&broadcast_response("lc-live")));
        server.reply_always(CHAT_MESSAGES, response);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let poller = YouTubeChatPoller::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token_source(),
            tx,
            "UCtest".to_owned(),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
        .with_timings(ChatPollTimings {
            broadcast_cadence: cadence,
            ..FAST
        });
        let cancel = CancellationToken::new();
        let join = tokio::spawn(poller.run(cancel.clone()));

        let rediscovered = wait_until(|| server.request_count(BROADCASTS) >= 2).await;
        cancel.cancel();
        join.await.unwrap().unwrap();

        assert!(rediscovered, "{label}: discovery never resumed");
        let discoveries = server.arrivals_of(BROADCASTS);
        assert_eq!(
            (
                server.answered_paths().get(..3).map(<[String]>::to_vec),
                discoveries[1] - discoveries[0] >= cadence
            ),
            (
                Some(vec![
                    BROADCASTS.to_owned(),
                    CHAT_MESSAGES.to_owned(),
                    BROADCASTS.to_owned()
                ]),
                true
            ),
            "{label}: (request order, next discovery a full cadence after the first)"
        );
    }
}

#[tokio::test]
async fn same_live_session_resolved_twice_emits_single_online() {
    let server = StubServer::start().await;
    reply_sequenced_broadcasts(
        &server,
        &broadcast_response("lc-1"),
        &broadcast_response("lc-2"),
    );

    let (poller, resolved, rx) = stub_poller_with_quota(&server, quota_for_two_resolutions());
    let events = drain_events_over_broadcast_cycles(poller, rx, &resolved, 2).await;

    let online = events
        .iter()
        .filter(|e| e.kind == "youtube.stream.online")
        .count();
    assert_eq!(
        online, 1,
        "an uninterrupted live session must emit online once, got {events:?}"
    );
}

const URL_SENTINEL: &str = "yt-url-leak-probe";

fn poller_at(api_base: String) -> YouTubeChatPoller {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_at(&api_base),
        token_source(),
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    )
}

fn failed_with<T>(result: Result<T, FetchFailure>) -> PlatformError {
    match result {
        Err(FetchFailure::Failed(error)) => error,
        Err(FetchFailure::Cancelled) => panic!("expected a failure, got Cancelled"),
        Err(FetchFailure::QuotaExhausted) => panic!("expected a failure, got QuotaExhausted"),
        Err(FetchFailure::ChatEnded) => panic!("expected a failure, got ChatEnded"),
        Ok(_) => panic!("expected the fetch to fail"),
    }
}

async fn failures_from_both_fetches(poller: &YouTubeChatPoller) -> [PlatformError; 2] {
    let cancel = CancellationToken::new();
    [
        failed_with(poller.fetch_live_chat_id("tok", &cancel).await),
        failed_with(
            poller
                .fetch_chat_messages("tok", "lc-1", None, &cancel)
                .await,
        ),
    ]
}

#[tokio::test]
async fn refused_connection_reports_a_network_reason_stripped_of_the_request_url() {
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = closed.local_addr().unwrap().to_string();
    drop(closed);
    let poller = poller_at(format!("http://{host}/{URL_SENTINEL}"));

    for failure in failures_from_both_fetches(&poller).await {
        let PlatformError::Network { reason } = failure else {
            panic!("expected Network, got {failure:?}");
        };
        assert!(
            !reason.contains(URL_SENTINEL) && !reason.contains(&host) && !reason.is_empty(),
            "the transport failure must be described without the request URL: {reason:?}"
        );
    }
}

#[tokio::test]
async fn undecodable_body_reports_a_malformed_response_stripped_of_the_request_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("<html>not json</html>"),
        )
        .mount(&server)
        .await;

    let poller = poller_at(format!("{}/{URL_SENTINEL}", server.uri()));

    for failure in failures_from_both_fetches(&poller).await {
        let PlatformError::MalformedResponse { reason } = failure else {
            panic!("expected MalformedResponse, got {failure:?}");
        };
        assert!(
            !reason.contains(URL_SENTINEL) && !reason.contains(&server.uri()),
            "request url leaked into the decode error: {reason:?}"
        );
    }
}

fn fixture_line(edit: impl FnOnce(&mut serde_json::Value)) -> Event {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/published_chat_message.json"
    ))
    .unwrap();
    let mut raw = fixture["raw"].clone();
    edit(&mut raw);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::unreachable_data_api(),
        token_source(),
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    );
    poller
        .build_event(&raw, &mut DedupSet::bounded(DEDUP_WINDOW_SIZE))
        .unwrap()
}

#[test]
fn a_message_whose_author_has_no_channel_id_carries_no_viewer_identity() {
    for channel_id in [json!(null), json!("")] {
        let ev = fixture_line(|raw| raw["authorDetails"]["channelId"] = channel_id.clone());
        assert!(ChatViewer::read(&ev.payload).is_none(), "{channel_id}");
    }
}

#[test]
fn a_blank_or_missing_profile_image_publishes_no_avatar() {
    for (case, edit) in [("blank", json!("")), ("null", json!(null))] {
        let ev = fixture_line(|raw| raw["authorDetails"]["profileImageUrl"] = edit.clone());
        assert_eq!(ev.payload["author_avatar_url"], json!(null), "{case}");
    }
    let ev = fixture_line(|raw| {
        raw["authorDetails"]
            .as_object_mut()
            .map(|details| details.remove("profileImageUrl"));
    });
    assert_eq!(ev.payload["author_avatar_url"], json!(null), "absent");
}

#[test]
fn a_text_message_publishes_exactly_the_shared_fixture_payload() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/published_chat_message.json"
    ))
    .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::unreachable_data_api(),
        token_source(),
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    );

    let ev = poller
        .build_event(&fixture["raw"], &mut DedupSet::bounded(DEDUP_WINDOW_SIZE))
        .unwrap();

    let actual = json!({ "source": ev.source, "kind": ev.kind, "payload": ev.payload });
    let expected = json!({
        "source": fixture["source"],
        "kind": fixture["kind"],
        "payload": fixture["payload"],
    });
    assert_eq!(
        actual,
        expected,
        "published chat event drifted from the fixture; actual:\n{}",
        serde_json::to_string_pretty(&actual).unwrap()
    );
}

#[derive(Default)]
struct ProbeBanLedger {
    upserts: std::sync::Mutex<Vec<forge_storage::BanLedgerEntry>>,
    entered: tokio::sync::Notify,
    release: Option<tokio::sync::Notify>,
}

impl ProbeBanLedger {
    fn gated() -> Self {
        Self {
            release: Some(tokio::sync::Notify::new()),
            ..Self::default()
        }
    }

    fn upserts(&self) -> Vec<forge_storage::BanLedgerEntry> {
        self.upserts.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl BanLedgerRepo for ProbeBanLedger {
    async fn upsert(
        &self,
        entry: &forge_storage::BanLedgerEntry,
        _now: OffsetDateTime,
    ) -> Result<forge_storage::BanLedgerEntry, forge_storage::StorageError> {
        self.upserts.lock().unwrap().push(entry.clone());
        self.entered.notify_one();
        if let Some(release) = &self.release {
            release.notified().await;
        }
        Ok(entry.clone())
    }

    async fn remove(
        &self,
        _key: &forge_storage::BanLedgerKey,
    ) -> Result<bool, forge_storage::StorageError> {
        Ok(false)
    }

    async fn get(
        &self,
        _key: &forge_storage::BanLedgerKey,
        _now: OffsetDateTime,
    ) -> Result<Option<forge_storage::BanLedgerEntry>, forge_storage::StorageError> {
        Ok(None)
    }

    async fn list_active(
        &self,
        _platform: forge_types::PlatformId,
        _channel_id: &str,
        _now: OffsetDateTime,
        _limit: usize,
    ) -> Result<Vec<forge_storage::BanLedgerEntry>, forge_storage::StorageError> {
        Ok(Vec::new())
    }
}

async fn next_event(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Event>) -> Event {
    tokio::time::timeout(Duration::from_secs(30), rx.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn observed_ban_is_recorded_before_the_event_is_published() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-ban-order")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([banned_item("ban-order", "Troll", "permanent", json!(null))]),
            3000,
        ),
    )
    .await;
    let ledger = Arc::new(ProbeBanLedger::gated());
    let (poller, mut rx) =
        make_poller_with_ledger(&server, Arc::clone(&ledger) as Arc<dyn BanLedgerRepo>);
    let cancel = CancellationToken::new();
    let handle = tokio::spawn({
        let cancel = cancel.clone();
        async move { poller.run(cancel).await }
    });
    assert_leading_online(&mut rx).await;

    tokio::time::timeout(Duration::from_secs(30), ledger.entered.notified())
        .await
        .expect("the observed ban must reach the ledger");
    let published_while_recording = rx.try_recv().ok().map(|event| event.kind);
    ledger.release.as_ref().unwrap().notify_one();
    assert_eq!(published_while_recording, None);
    let event = next_event(&mut rx).await;
    cancel.cancel();
    handle.await.unwrap().unwrap();

    assert_eq!(event.kind, "youtube.channel.user_banned");
}

#[tokio::test]
async fn observed_ban_is_recorded_under_the_poller_channel() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-ban-row")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([banned_item("ban-row", "Troll", "temporary", json!(600))]),
            3000,
        ),
    )
    .await;
    let ledger = Arc::new(ProbeBanLedger::default());
    let (poller, mut rx) =
        make_poller_with_ledger(&server, Arc::clone(&ledger) as Arc<dyn BanLedgerRepo>);
    let cancel = CancellationToken::new();
    let handle = tokio::spawn({
        let cancel = cancel.clone();
        async move { poller.run(cancel).await }
    });
    assert_leading_online(&mut rx).await;
    next_event(&mut rx).await;
    cancel.cancel();
    handle.await.unwrap().unwrap();

    let keys: Vec<(String, String)> = ledger
        .upserts()
        .into_iter()
        .map(|entry| (entry.key.channel_id, entry.key.viewer_id))
        .collect();
    assert_eq!(keys, [("UCtest".to_owned(), "UCbanned".to_owned())]);
}

#[tokio::test]
async fn observed_ban_is_published_when_the_ledger_write_fails() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-ban-fail")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([banned_item("ban-fail", "Troll", "permanent", json!(null))]),
            3000,
        ),
    )
    .await;
    let (poller, mut rx) = make_poller_with_ledger(
        &server,
        Arc::new(
            crate::chat::ban_ledger_test_support::MemoryBanLedger::failing_on(
                crate::chat::ban_ledger_test_support::LedgerOp::Upsert,
            ),
        ),
    );
    let cancel = CancellationToken::new();
    let handle = tokio::spawn({
        let cancel = cancel.clone();
        async move { poller.run(cancel).await }
    });
    assert_leading_online(&mut rx).await;
    let event = next_event(&mut rx).await;
    cancel.cancel();
    handle.await.unwrap().unwrap();

    assert_eq!(event.kind, "youtube.channel.user_banned");
}

#[tokio::test]
async fn duplicate_ban_item_is_published_and_recorded_once() {
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-ban-dup")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([
                banned_item("ban-dup", "Troll", "permanent", json!(null)),
                banned_item("ban-dup", "Troll", "permanent", json!(null)),
                text_item("after-dup", "still here", "Viewer"),
            ]),
            3000,
        ),
    )
    .await;
    let ledger = Arc::new(ProbeBanLedger::default());
    let (poller, mut rx) =
        make_poller_with_ledger(&server, Arc::clone(&ledger) as Arc<dyn BanLedgerRepo>);
    let cancel = CancellationToken::new();
    let handle = tokio::spawn({
        let cancel = cancel.clone();
        async move { poller.run(cancel).await }
    });
    assert_leading_online(&mut rx).await;
    let mut kinds = Vec::new();
    loop {
        let event = next_event(&mut rx).await;
        let is_marker = event.kind != "youtube.channel.user_banned";
        kinds.push(event.kind);
        if is_marker {
            break;
        }
    }
    cancel.cancel();
    handle.await.unwrap().unwrap();

    assert_eq!(
        (kinds.len(), ledger.upserts().len()),
        (2, 1),
        "events seen: {kinds:?}"
    );
}

use crate::api::poll_health::PollLinkHealth;

type HealthReport = (PollLinkHealth, usize);

struct ProbedPoller {
    poller: YouTubeChatPoller,
    reports: tokio::sync::mpsc::UnboundedReceiver<HealthReport>,
    token_calls: Arc<std::sync::atomic::AtomicUsize>,
    _events: tokio::sync::mpsc::UnboundedReceiver<Event>,
}

enum TokenOutcome {
    Issued,
    Refused,
}

fn probed_poller(server: &StubServer, token: TokenOutcome, quota: SharedQuota) -> ProbedPoller {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let token_calls = Arc::clone(&calls);
    let counter = Arc::clone(&calls);
    let issues = matches!(token, TokenOutcome::Issued);
    let source: TokenSource = Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            if issues {
                Ok("test-token".to_owned())
            } else {
                Err(PlatformError::ReauthRequired {
                    platform: "youtube".to_owned(),
                })
            }
        })
    });
    let (report_tx, reports) = tokio::sync::mpsc::unbounded_channel();
    let reporter: PollHealthReporter = Arc::new(move |health| {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = report_tx.send((health, calls.load(std::sync::atomic::Ordering::SeqCst)));
    });
    let (tx, events) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_charging(&server.uri(), quota),
        source,
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    )
    .with_health_reporter(reporter)
    .with_timings(FAST);
    ProbedPoller {
        poller,
        reports,
        token_calls,
        _events: events,
    }
}

async fn first_reports(probed: ProbedPoller, count: usize) -> Vec<HealthReport> {
    let ProbedPoller {
        poller,
        mut reports,
        _events,
        ..
    } = probed;
    let cancel = CancellationToken::new();
    let join = tokio::spawn(poller.run(cancel.clone()));
    let mut seen = Vec::new();
    let reached = wait_until(|| {
        while let Ok(report) = reports.try_recv() {
            seen.push(report);
        }
        seen.len() >= count
    })
    .await;
    assert!(reached, "only {} health report(s): {seen:?}", seen.len());
    cancel.cancel();
    join.await.unwrap().unwrap();
    seen
}

fn quota_with_room_for(units: u32) -> SharedQuota {
    SharedQuota::new(QuotaState {
        used_today: QUOTA_DAILY_LIMIT_FOR_TEST - units,
        last_reset_date: today_pacific(),
    })
}

fn server_error() -> StubReply {
    StubReply::status(reqwest::StatusCode::INTERNAL_SERVER_ERROR.as_u16())
}

fn live_chat_ended() -> StubReply {
    StubReply::json(
        reqwest::StatusCode::FORBIDDEN.as_u16(),
        &json!({ "error": { "errors": [{ "reason": LIVE_CHAT_ENDED_REASON }] } }),
    )
}

fn reply_then(server: &StubServer, path: &str, first: StubReply, times: usize, then: StubReply) {
    server.reply_times(path, first, times);
    server.reply_always(path, then);
}

const LIVE_LOOKUP_CALLS: usize = 1;
const NEXT_CALL: usize = 1;
const LOSS_THRESHOLD: usize = crate::api::poll_health::CONSECUTIVE_POLL_FAILURES_FOR_LOSS as usize;

const ONE_LONG_ROUND: ChatPollTimings = ChatPollTimings {
    broadcast_cadence: crate::endpoints_test_support::HANG_GUARD,
    ..FAST
};

fn live_broadcast() -> StubReply {
    StubReply::ok(&broadcast_response("lc-live"))
}

#[tokio::test]
async fn each_failing_poll_step_reports_loss_on_the_third_consecutive_failure() {
    for (label, token, broadcasts, chat, timings, expected) in [
        (
            "token refused",
            TokenOutcome::Refused,
            server_error(),
            server_error(),
            FAST,
            vec![(PollLinkHealth::Lost, LOSS_THRESHOLD)],
        ),
        (
            "broadcast lookup failing",
            TokenOutcome::Issued,
            server_error(),
            server_error(),
            FAST,
            vec![(PollLinkHealth::Lost, LOSS_THRESHOLD)],
        ),
        (
            "chat request failing",
            TokenOutcome::Issued,
            live_broadcast(),
            server_error(),
            ONE_LONG_ROUND,
            vec![
                (PollLinkHealth::Healthy, LIVE_LOOKUP_CALLS),
                (PollLinkHealth::Lost, LIVE_LOOKUP_CALLS + LOSS_THRESHOLD),
            ],
        ),
    ] {
        let server = StubServer::start().await;
        server.reply_always(BROADCASTS, broadcasts);
        server.reply_always(CHAT_MESSAGES, chat);
        let mut probed = probed_poller(&server, token, make_quota());
        probed.poller = probed.poller.with_timings(timings);
        let reports = first_reports(probed, expected.len()).await;
        assert_eq!(reports, expected, "{label}");
    }
}

#[tokio::test]
async fn absent_broadcast_after_a_loss_reports_recovery() {
    let server = StubServer::start().await;
    reply_then(
        &server,
        BROADCASTS,
        server_error(),
        LOSS_THRESHOLD,
        StubReply::ok(&empty_broadcast_response()),
    );
    let reports = first_reports(
        probed_poller(&server, TokenOutcome::Issued, make_quota()),
        2,
    )
    .await;
    assert_eq!(
        reports,
        [
            (PollLinkHealth::Lost, LOSS_THRESHOLD),
            (PollLinkHealth::Healthy, LOSS_THRESHOLD + 1),
        ]
    );
}

#[tokio::test]
async fn live_chat_ended_counts_as_a_successful_poll() {
    let server = StubServer::start().await;
    reply_then(
        &server,
        BROADCASTS,
        live_broadcast(),
        LIVE_LOOKUP_CALLS,
        server_error(),
    );
    reply_then(
        &server,
        CHAT_MESSAGES,
        server_error(),
        LOSS_THRESHOLD,
        live_chat_ended(),
    );
    let lost_at_call = LIVE_LOOKUP_CALLS + LOSS_THRESHOLD;
    let mut probed = probed_poller(&server, TokenOutcome::Issued, make_quota());
    probed.poller = probed.poller.with_timings(ONE_LONG_ROUND);
    let reports = first_reports(probed, 3).await;
    assert_eq!(
        reports,
        [
            (PollLinkHealth::Healthy, LIVE_LOOKUP_CALLS),
            (PollLinkHealth::Lost, lost_at_call),
            (PollLinkHealth::Healthy, lost_at_call + NEXT_CALL),
        ]
    );
}

async fn ids_once_broadcast_lookup_reaches(
    lookups: usize,
    after_first: StubReply,
) -> (Option<String>, Option<String>) {
    let server = StubServer::start().await;
    reply_then(
        &server,
        BROADCASTS,
        live_broadcast(),
        LIVE_LOOKUP_CALLS,
        after_first,
    );
    server.reply_always(
        CHAT_MESSAGES,
        StubReply::ok(&json!({ "pollingIntervalMillis": 0, "items": [] })),
    );
    let live = LiveChatIdHandle::new();
    let broadcast = ActiveBroadcastIdHandle::new();
    let (tx, _events) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_charging(&server.uri(), make_quota()),
        token_source(),
        tx,
        "UCtest".to_owned(),
        live.clone(),
        broadcast.clone(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    )
    .with_timings(FAST);
    let cancel = CancellationToken::new();
    let join = tokio::spawn(poller.run(cancel.clone()));

    let reached = wait_until(|| server.request_count(BROADCASTS) >= lookups).await;
    let ids = (live.get(), broadcast.get());
    cancel.cancel();
    join.await.unwrap().unwrap();
    assert!(reached, "the poller never re-resolved the broadcast");
    ids
}

#[tokio::test]
async fn ids_stay_published_while_the_broadcast_is_re_resolved() {
    let ids = ids_once_broadcast_lookup_reaches(LIVE_LOOKUP_CALLS + 1, StubReply::held()).await;
    assert_eq!(
        ids,
        (Some("lc-live".to_owned()), Some("broadcast-1".to_owned()))
    );
}

#[tokio::test]
async fn ids_stay_published_through_a_transient_broadcast_lookup_failure() {
    let ids = ids_once_broadcast_lookup_reaches(LIVE_LOOKUP_CALLS + 2, server_error()).await;
    assert_eq!(
        ids,
        (Some("lc-live".to_owned()), Some("broadcast-1".to_owned()))
    );
}

#[tokio::test]
async fn a_local_quota_refusal_is_not_counted_as_a_failed_poll() {
    let server = StubServer::start().await;
    server.reply_always(BROADCASTS, server_error());
    let failed_lookups = LOSS_THRESHOLD - 1;
    let room = u32::try_from(failed_lookups).unwrap() * BROADCAST_COST;
    let ProbedPoller {
        poller,
        mut reports,
        token_calls,
        _events,
    } = probed_poller(&server, TokenOutcome::Issued, quota_with_room_for(room));
    let cancel = CancellationToken::new();
    let join = tokio::spawn(poller.run(cancel.clone()));
    let past_two_refusals = failed_lookups + 3;

    let reached =
        wait_until(|| token_calls.load(std::sync::atomic::Ordering::SeqCst) >= past_two_refusals)
            .await;
    cancel.cancel();
    join.await.unwrap().unwrap();

    assert_eq!(
        (
            reached,
            reports.try_recv().ok(),
            server.request_count(BROADCASTS)
        ),
        (true, None, failed_lookups)
    );
}

#[tokio::test]
async fn first_chat_poll_after_a_pacific_rollover_waits_only_the_short_floor() {
    let server = StubServer::start().await;
    server.reply_always(BROADCASTS, StubReply::ok(&broadcast_response("lc-live")));
    server.reply_always(
        CHAT_MESSAGES,
        StubReply::ok(&chat_response(
            json!([text_item("m-1", "hello", "Viewer")]),
            0,
        )),
    );
    let previous_day = time::Date::from_calendar_date(2020, time::Month::January, 1).unwrap();
    let quota = SharedQuota::new(QuotaState {
        used_today: QUOTA_DAILY_LIMIT_FOR_TEST - 500,
        last_reset_date: previous_day,
    });
    let (token_source, token_calls) = counting_token_source();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        crate::endpoints_test_support::data_api_charging(&server.uri(), quota),
        token_source,
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    )
    .with_timings(ChatPollTimings {
        poll_floor: Duration::from_millis(1),
        long_interval: crate::endpoints_test_support::HANG_GUARD * 2,
        broadcast_cadence: crate::endpoints_test_support::HANG_GUARD * 2,
    });
    let cancel = CancellationToken::new();
    let join = tokio::spawn(poller.run(cancel.clone()));

    let second_chat_poll =
        wait_until(|| token_calls.load(std::sync::atomic::Ordering::SeqCst) >= 3).await;
    cancel.cancel();
    join.await.unwrap().unwrap();

    assert!(
        second_chat_poll,
        "discovery + first chat poll + a second chat poll after the short floor"
    );
}

#[test]
fn cancelling_a_poll_mid_fetch_returns_quietly_without_charging_or_reporting_health() {
    for (label, held_path, charged, expected_reports) in [
        ("broadcast lookup", BROADCASTS, 0, Vec::new()),
        (
            "chat poll",
            CHAT_MESSAGES,
            BROADCAST_COST,
            vec![PollLinkHealth::Healthy],
        ),
    ] {
        let quota = SharedQuota::default();
        let observed = quota.clone();
        let (outcome, lines) =
            forge_test_support::log_capture::capture_blocking(tracing::Level::WARN, async move {
                let server = StubServer::start().await;
                server.reply_always(BROADCASTS, live_broadcast());
                server.reply_always(held_path, StubReply::held());
                let ProbedPoller {
                    poller,
                    mut reports,
                    _events,
                    ..
                } = probed_poller(&server, TokenOutcome::Issued, quota);
                let cancel = CancellationToken::new();
                let join = tokio::spawn(poller.run(cancel.clone()));
                forge_test_support::settle_until("the held request reached the server", || {
                    server.request_count(held_path) == 1
                })
                .await;

                cancel.cancel();
                let finished = join.await.unwrap();

                (
                    finished.is_ok(),
                    std::iter::from_fn(|| reports.try_recv().ok())
                        .map(|(health, _)| health)
                        .collect::<Vec<_>>(),
                    used_today(&observed).await,
                )
            });

        assert_eq!(
            (
                outcome,
                forge_test_support::log_capture::forge_lines(&lines).len()
            ),
            ((true, expected_reports, charged), 0),
            "{label}"
        );
    }
}

#[tokio::test]
async fn a_throttle_seen_by_another_client_pauses_the_broadcast_lookup() {
    let server = StubServer::start().await;
    server.reply_always(
        CHAT_MESSAGES,
        StubReply::status(reqwest::StatusCode::TOO_MANY_REQUESTS.as_u16())
            .header("retry-after", "3600"),
    );
    server.reply_always(BROADCASTS, live_broadcast());
    let quota = SharedQuota::default();
    let api = crate::endpoints_test_support::data_api_charging(&server.uri(), quota.clone());
    let live = LiveChatIdHandle::new();
    live.set(Some("lc-live".to_owned()));
    let sender = crate::chat::send_chat::YouTubeSendChat::new(api.clone(), token_source(), live);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let poller = YouTubeChatPoller::new(
        api,
        token_source(),
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    );
    let throttled = sender.send("hi").await;
    let charged_by_send = used_today(&quota).await;

    let lookup = poller
        .fetch_live_chat_id("tok", &CancellationToken::new())
        .await;

    assert!(
        matches!(throttled, Err(PlatformError::RateLimited { .. })),
        "{throttled:?}"
    );
    assert!(
        matches!(failed_with(lookup), PlatformError::RateLimitExhausted),
        "the lookup must be refused locally"
    );
    assert_eq!(
        (server.request_count(BROADCASTS), used_today(&quota).await),
        (0, charged_by_send)
    );
}

fn google_refusal(status: reqwest::StatusCode, reason: &str) -> ResponseTemplate {
    ResponseTemplate::new(status.as_u16()).set_body_json(json!({
        "error": {
            "code": status.as_u16(),
            "message": "refused",
            "errors": [{ "domain": "youtube.liveChat", "reason": reason, "message": "refused" }]
        }
    }))
}

#[tokio::test]
async fn only_a_403_naming_live_chat_ended_is_kept_out_of_request_fail() {
    let cases = [
        (
            google_refusal(reqwest::StatusCode::FORBIDDEN, LIVE_CHAT_ENDED_REASON),
            true,
            0,
        ),
        (
            google_refusal(reqwest::StatusCode::FORBIDDEN, "liveChatDisabled"),
            false,
            1,
        ),
        (
            google_refusal(reqwest::StatusCode::NOT_FOUND, LIVE_CHAT_ENDED_REASON),
            false,
            1,
        ),
    ];

    for (reply, ended, request_fails) in cases {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(MESSAGES_PATH))
            .respond_with(reply)
            .mount(&server)
            .await;
        let bus = Arc::new(CapturingPublisher::default());
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let poller = YouTubeChatPoller::new(
            data_api_with(
                &server.uri(),
                SharedQuota::default(),
                Arc::clone(&bus) as Arc<dyn forge_events::EventPublisher>,
            ),
            token_source(),
            tx,
            "UCtest".to_owned(),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
        );

        let outcome = poller
            .fetch_chat_messages("tok", "lc-1", None, &CancellationToken::new())
            .await;

        assert_eq!(
            (
                matches!(outcome, Err(FetchFailure::ChatEnded)),
                bus.of_kind(REQUEST_FAIL_KIND).len()
            ),
            (ended, request_fails)
        );
    }
}

fn token_source_hanging_from_call(
    hang_from: usize,
) -> (TokenSource, Arc<std::sync::atomic::AtomicUsize>) {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let source: TokenSource = Arc::new(move || {
        let call = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        Box::pin(async move {
            if call >= hang_from {
                std::future::pending::<()>().await;
            }
            Ok("test-token".to_owned())
        })
    });
    (source, calls)
}

#[tokio::test]
async fn cancelling_while_the_access_token_is_being_fetched_returns_at_once() {
    for (label, hang_from) in [("broadcast resolution", 1), ("chat poll", 2)] {
        let server = StubServer::start().await;
        server.reply_always(BROADCASTS, live_broadcast());
        server.reply_always(
            CHAT_MESSAGES,
            StubReply::ok(&chat_response(json!([]), POLL_FLOOR_MS)),
        );
        let (source, calls) = token_source_hanging_from_call(hang_from);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let poller = YouTubeChatPoller::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            source,
            tx,
            "UCtest".to_owned(),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
        );
        let cancel = CancellationToken::new();
        let join = tokio::spawn(poller.run(cancel.clone()));
        forge_test_support::settle_until("the poller asked for the hanging token", || {
            calls.load(std::sync::atomic::Ordering::SeqCst) == hang_from
        })
        .await;

        cancel.cancel();
        let finished = tokio::time::timeout(crate::endpoints_test_support::HANG_GUARD, join).await;

        assert!(matches!(finished, Ok(Ok(Ok(())))), "{label}");
    }
}

fn item_by(id: &str, channel_id: &str, profile_image: Option<&str>) -> serde_json::Value {
    let mut item = text_item(id, "hi", "Viewer");
    item["authorDetails"]["channelId"] = json!(channel_id);
    if let Some(image) = profile_image {
        item["authorDetails"]["profileImageUrl"] = json!(image);
    }
    item
}

#[tokio::test]
async fn polled_chat_records_each_authors_profile_image_for_avatar_lookups() {
    use forge_platform_core::ViewerAvatarSource;

    const IMAGE: &str = "https://yt3.ggpht.com/ytc/viewer=s88";
    let server = MockServer::start().await;
    mount_broadcast_mock(&server, broadcast_response("chat-avatars")).await;
    mount_chat_mock(
        &server,
        chat_response(
            json!([
                item_by("msg-1", "UCpictured", Some(IMAGE)),
                item_by("msg-2", "UCplain", None),
            ]),
            3000,
        ),
    )
    .await;
    let book = crate::chat::avatar_book::AvatarBook::default();
    let (poller, mut rx) = make_poller_with_receiver(&server);
    let poller = poller.with_avatar_book(book.clone());
    let cancel = CancellationToken::new();
    let handle = tokio::spawn({
        let cancel = cancel.clone();
        async move { poller.run(cancel).await }
    });

    assert_leading_online(&mut rx).await;
    for _ in 0..2 {
        tokio::time::timeout(Duration::from_secs(30), rx.recv())
            .await
            .unwrap()
            .unwrap();
    }
    cancel.cancel();
    handle.await.unwrap().unwrap();

    assert_eq!(
        (book.remembered("UCpictured"), book.remembered("UCplain")),
        (Some(IMAGE.to_owned()), None)
    );
}

#[test]
fn a_rest_page_reads_its_polling_interval_from_a_number_or_a_numeric_string() {
    for (case, body, expected) in [
        ("number", json!({ "pollingIntervalMillis": 2500 }), 2500),
        ("string", json!({ "pollingIntervalMillis": "2500" }), 2500),
        ("missing", json!({}), DEFAULT_POLLING_INTERVAL_MS),
        (
            "non-numeric",
            json!({ "pollingIntervalMillis": "soon" }),
            DEFAULT_POLLING_INTERVAL_MS,
        ),
    ] {
        assert_eq!(
            fetch::page_from_body(&body).polling_interval_millis,
            expected,
            "{case}"
        );
    }
}

#[test]
fn a_rest_page_carries_its_items_and_next_page_token() {
    let page = fetch::page_from_body(&json!({
        "items": [{ "id": "a" }, { "id": "b" }],
        "nextPageToken": "page-2"
    }));
    assert_eq!(page.items, vec![json!({ "id": "a" }), json!({ "id": "b" })]);
    assert_eq!(page.next_page_token.as_deref(), Some("page-2"));
}

#[test]
fn a_rest_page_without_items_or_token_reads_as_empty() {
    let page = fetch::page_from_body(&json!({ "pollingIntervalMillis": 2500 }));
    assert!(page.items.is_empty());
    assert_eq!(page.next_page_token, None);
}
