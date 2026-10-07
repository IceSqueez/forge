#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use forge_emulator::EmulatorError;
use forge_emulator::fixture::{REDACTED, YouTubeAccount};
use forge_emulator::youtube::{
    FakeYouTube, FakeYouTubeConfig, FakeYouTubeSetup, LIVE_BROADCASTS_PATH,
    LIVE_CHAT_MESSAGES_PATH, TOKEN_PATH, YouTubeChatter, YouTubeCredentialCheck,
    YouTubeRefreshAnswer, YouTubeRequest, YouTubeSubscriber,
};
use forge_events::Event;
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError, ViewerReport};
use forge_platform_youtube::{
    ActiveBroadcastIdHandle, GoogleAuthFlow, LiveChatIdHandle, SharedQuota, YoutubeAuthBundle,
    YoutubeChatPoller, YoutubeCredentialsManager, YoutubeSendChat, YoutubeViewerPoll,
};
use forge_storage::{CredentialId, CredentialsRepo, StorageError};
use forge_types::OAuthToken;
use futures_util::future::BoxFuture;
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(10);
const CLIENT_ID: &str = "emulator-youtube-client";
const CLIENT_SECRET: &str = "emulator-youtube-client-secret";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

struct CancelOnDrop(JoinHandle<()>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.abort();
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
        Ok(Vec::new())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

async fn start(setup: FakeYouTubeSetup) -> FakeYouTube {
    FakeYouTube::start(FakeYouTubeConfig::for_account(
        &YouTubeAccount::default(),
        &setup,
    ))
    .await
    .unwrap()
}

async fn start_live() -> FakeYouTube {
    start(FakeYouTubeSetup {
        live_at_boot: true,
        ..FakeYouTubeSetup::default()
    })
    .await
}

fn endpoints(fake: &FakeYouTube) -> PlatformEndpoints {
    let overrides = fake.endpoint_overrides();
    PlatformEndpoints::resolve(|variable| {
        overrides
            .iter()
            .find(|(name, _)| *name == variable)
            .map(|(_, url)| OsString::from(url))
    })
    .unwrap()
}

fn data_api(fake: &FakeYouTube, route: &str) -> String {
    format!(
        "{}{route}",
        endpoints(fake).base_url(EndpointSurface::YouTubeDataApi)
    )
}

fn token_source(token: String) -> TokenSource {
    Arc::new(move || {
        let token = token.clone();
        Box::pin(async move { Ok(token) })
    })
}

fn quota() -> SharedQuota {
    SharedQuota::default()
}

fn alice() -> YouTubeChatter {
    YouTubeChatter {
        channel_id: "UCaliceViewer00000000001".to_owned(),
        display_name: "Alice".to_owned(),
        sponsor: false,
        moderator: false,
    }
}

async fn get(fake: &FakeYouTube, route: &str, query: &[(&str, &str)]) -> (u16, Value) {
    let response = reqwest::Client::new()
        .get(data_api(fake, route))
        .bearer_auth(fake.access_token())
        .query(query)
        .send()
        .await
        .unwrap();
    (response.status().as_u16(), response.json().await.unwrap())
}

async fn insert(fake: &FakeYouTube, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(data_api(fake, "/liveChat/messages"))
        .bearer_auth(fake.access_token())
        .query(&[("part", "snippet")])
        .json(&body)
        .send()
        .await
        .unwrap();
    (response.status().as_u16(), response.json().await.unwrap())
}

fn reason(body: &Value) -> &str {
    body.pointer("/error/errors/0/reason")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn last_request(fake: &FakeYouTube) -> YouTubeRequest {
    fake.ledger().requests.last().cloned().unwrap()
}

async fn list_chat(fake: &FakeYouTube, page_token: Option<&str>) -> (u16, Value) {
    let live_chat_id = fake.config().live_chat_id.clone();
    let mut query = vec![
        ("liveChatId", live_chat_id.as_str()),
        ("part", "snippet,authorDetails"),
    ];
    if let Some(token) = page_token {
        query.push(("pageToken", token));
    }
    get(fake, "/liveChat/messages", &query).await
}

struct ForgePoller {
    events: mpsc::UnboundedReceiver<Event>,
    _task: CancelOnDrop,
}

impl ForgePoller {
    fn start(fake: &FakeYouTube) -> Self {
        let (tx, events) = mpsc::unbounded_channel();
        let poller = YoutubeChatPoller::new(
            &endpoints(fake),
            token_source(fake.access_token()),
            tx,
            fake.config().channel_id.clone(),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            quota(),
            Arc::new(forge_storage::ban_ledger::MockBanLedgerRepo::new()),
        );
        let cancel = tokio_util::sync::CancellationToken::new();
        let task = tokio::spawn(async move {
            poller.run(cancel).await.ok();
        });
        Self {
            events,
            _task: CancelOnDrop(task),
        }
    }

    async fn next_of(&mut self, kind: &str) -> Event {
        timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.unwrap();
                if event.kind == kind {
                    return event;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("forge's poller published no {kind}"))
    }
}

async fn chat_polled(fake: &FakeYouTube) {
    fake.wait_for("a chat poll", WAIT, |ledger| {
        ledger
            .requests
            .iter()
            .any(|request| request.path == LIVE_CHAT_MESSAGES_PATH && request.status == 200)
            .then_some(())
    })
    .await
    .unwrap();
}

async fn manager(
    fake: &FakeYouTube,
    expires_at: SystemTime,
    handle: Option<&str>,
) -> YoutubeCredentialsManager {
    let manager = YoutubeCredentialsManager::new(
        Arc::new(MemoryCredentials::default()),
        GoogleAuthFlow::new(
            &endpoints(fake),
            CLIENT_ID.to_owned(),
            CLIENT_SECRET.to_owned(),
        ),
    );
    let account = YouTubeAccount::default();
    manager
        .save_from_bundle(YoutubeAuthBundle {
            access_token: OAuthToken::new(account.access_token),
            refresh_token: OAuthToken::new(account.refresh_token),
            channel_id: account.channel_id,
            channel_title: account.channel_title,
            channel_handle: handle.map(ToOwned::to_owned),
            client_id: CLIENT_ID.to_owned(),
            expires_at,
        })
        .await
        .unwrap();
    manager
}

fn long_expired() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(60)
}

fn far_future() -> SystemTime {
    SystemTime::now() + Duration::from_secs(3600)
}

#[tokio::test]
async fn overrides_cover_every_youtube_surface_and_pass_forges_own_validation() {
    let fake = start(FakeYouTubeSetup::default()).await;
    let overridden: BTreeSet<EndpointSurface> = endpoints(&fake)
        .overridden()
        .map(|(surface, _)| surface)
        .collect();
    assert_eq!(
        overridden,
        BTreeSet::from([
            EndpointSurface::YouTubeDataApi,
            EndpointSurface::YouTubeUploadApi,
            EndpointSurface::YouTubeOAuth,
        ])
    );
}

#[tokio::test]
async fn blank_broadcast_or_chat_ids_are_refused_at_start() {
    for setup in [
        FakeYouTubeSetup {
            broadcast_id: " ".to_owned(),
            ..FakeYouTubeSetup::default()
        },
        FakeYouTubeSetup {
            live_chat_id: String::new(),
            ..FakeYouTubeSetup::default()
        },
    ] {
        let started = FakeYouTube::start(FakeYouTubeConfig::for_account(
            &YouTubeAccount::default(),
            &setup,
        ))
        .await;
        assert!(
            matches!(started, Err(EmulatorError::InvalidFakeConfig { .. })),
            "{setup:?}"
        );
    }
}

#[tokio::test]
async fn forge_poller_announces_the_live_broadcast_and_reads_an_injected_message() {
    let fake = start_live().await;
    let mut forge = ForgePoller::start(&fake);
    let online = forge.next_of("youtube.stream.online").await;
    chat_polled(&fake).await;

    fake.inject_chat_message(&alice(), "hello from the fake")
        .unwrap();
    let message = forge.next_of("youtube.chat.message").await;

    assert_eq!(
        (
            online.payload["broadcast_id"].as_str(),
            message.payload["message_text"].as_str(),
            message.payload["author"]["channel_id"].as_str(),
        ),
        (
            Some(fake.config().broadcast_id.as_str()),
            Some("hello from the fake"),
            Some(alice().channel_id.as_str()),
        )
    );
}

#[tokio::test]
async fn forge_poller_learns_the_end_from_the_chat_before_its_next_broadcast_poll() {
    let fake = start_live().await;
    let mut forge = ForgePoller::start(&fake);
    forge.next_of("youtube.stream.online").await;
    chat_polled(&fake).await;

    fake.set_live(false);
    forge.next_of("youtube.stream.offline").await;

    let broadcast_polls = fake
        .ledger()
        .requests
        .iter()
        .filter(|request| request.path == LIVE_BROADCASTS_PATH)
        .count();
    assert_eq!(broadcast_polls, 1);
}

#[tokio::test]
async fn next_page_token_serves_only_messages_added_after_the_previous_page() {
    let fake = start_live().await;
    fake.inject_chat_message(&alice(), "first").unwrap();
    let (_, first_page) = list_chat(&fake, None).await;
    let token = first_page["nextPageToken"].as_str().unwrap().to_owned();
    let (_, empty_page) = list_chat(&fake, Some(&token)).await;
    fake.inject_chat_message(&alice(), "second").unwrap();
    let (_, next_page) = list_chat(&fake, Some(&token)).await;

    let texts = |page: &Value| -> Vec<String> {
        page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                item["snippet"]["displayMessage"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    };
    assert_eq!(
        (texts(&first_page), texts(&empty_page), texts(&next_page)),
        (
            vec!["first".to_owned()],
            Vec::new(),
            vec!["second".to_owned()]
        )
    );
}

#[tokio::test]
async fn chat_list_carries_the_configured_polling_interval() {
    let fake = start(FakeYouTubeSetup {
        live_at_boot: true,
        polling_interval_ms: 4321,
        ..FakeYouTubeSetup::default()
    })
    .await;
    let (status, page) = list_chat(&fake, None).await;
    assert_eq!(
        (status, page["pollingIntervalMillis"].as_u64()),
        (200, Some(4321))
    );
}

#[tokio::test]
async fn chat_list_is_refused_with_the_documented_reason_for_each_unusable_chat() {
    let never_live = start(FakeYouTubeSetup::default()).await;
    let ended = start_live().await;
    ended.set_live(false);
    let (_, last_page) = list_chat(&ended, None).await;
    let past_end = last_page["nextPageToken"].as_str().unwrap().to_owned();
    let unknown = start_live().await;

    let cases = [
        (
            "never live",
            list_chat(&never_live, None).await,
            (403, "liveChatEnded"),
        ),
        (
            "ended and delivered",
            list_chat(&ended, Some(&past_end)).await,
            (403, "liveChatEnded"),
        ),
        (
            "unknown chat id",
            get(
                &unknown,
                "/liveChat/messages",
                &[("liveChatId", "other"), ("part", "snippet")],
            )
            .await,
            (404, "liveChatNotFound"),
        ),
        (
            "foreign page token",
            list_chat(&unknown, Some("not-ours")).await,
            (400, "badRequest"),
        ),
    ];
    for (label, (status, body), expected) in cases {
        assert_eq!((status, reason(&body)), expected, "{label}");
    }
}

#[tokio::test]
async fn ended_chat_delivers_chat_ended_event_with_offline_at() {
    let fake = start_live().await;
    fake.set_live(false);
    let (status, page) = list_chat(&fake, None).await;
    assert_eq!(
        (
            status,
            page["items"][0]["snippet"]["type"].as_str(),
            page["offlineAt"].is_string()
        ),
        (200, Some("chatEndedEvent"), true)
    );
}

#[tokio::test]
async fn injecting_into_an_offline_chat_is_refused() {
    let fake = start(FakeYouTubeSetup::default()).await;
    let refused = fake.inject_chat_message(&alice(), "anyone there?");
    assert!(
        matches!(refused, Err(EmulatorError::YouTubeChatNotLive { .. })),
        "{refused:?}"
    );
}

#[tokio::test]
async fn forge_send_inserts_a_text_message_that_comes_back_as_the_owner() {
    let fake = start_live().await;
    let live_chat = LiveChatIdHandle::new();
    live_chat.set(Some(fake.config().live_chat_id.clone()));
    let sender = YoutubeSendChat::new(
        &endpoints(&fake),
        token_source(fake.access_token()),
        live_chat,
        quota(),
    );

    sender.send("hello chat").await.unwrap();
    let (_, page) = list_chat(&fake, None).await;

    assert_eq!(
        (
            page["items"][0]["snippet"]["textMessageDetails"]["messageText"].as_str(),
            page["items"][0]["authorDetails"]["isChatOwner"].as_bool(),
        ),
        (Some("hello chat"), Some(true))
    );
}

#[tokio::test]
async fn forge_send_into_an_ended_chat_is_refused_as_live_chat_ended() {
    let fake = start_live().await;
    fake.set_live(false);
    let live_chat = LiveChatIdHandle::new();
    live_chat.set(Some(fake.config().live_chat_id.clone()));
    let sender = YoutubeSendChat::new(
        &endpoints(&fake),
        token_source(fake.access_token()),
        live_chat,
        quota(),
    );

    let refused = sender.send("too late").await;

    assert!(
        matches!(&refused, Err(PlatformError::Http { status: 403, body }) if body.contains("liveChatEnded")),
        "{refused:?}"
    );
}

#[tokio::test]
async fn malformed_inserts_are_refused_with_the_documented_reason() {
    let fake = start_live().await;
    let chat = fake.config().live_chat_id.clone();
    let cases = [
        (
            "no liveChatId",
            json!({ "snippet": { "type": "textMessageEvent", "textMessageDetails": { "messageText": "x" } } }),
            (400, "liveChatIdRequired"),
        ),
        (
            "another chat",
            json!({ "snippet": { "liveChatId": "other", "type": "textMessageEvent", "textMessageDetails": { "messageText": "x" } } }),
            (404, "liveChatNotFound"),
        ),
        (
            "no type",
            json!({ "snippet": { "liveChatId": chat, "textMessageDetails": { "messageText": "x" } } }),
            (400, "typeRequired"),
        ),
        (
            "blank text",
            json!({ "snippet": { "liveChatId": chat, "type": "textMessageEvent", "textMessageDetails": { "messageText": " " } } }),
            (400, "messageTextRequired"),
        ),
    ];
    for (label, body, expected) in cases {
        let (status, answer) = insert(&fake, body).await;
        assert_eq!((status, reason(&answer)), expected, "{label}");
    }
}

#[tokio::test]
async fn inserting_a_type_the_fake_does_not_model_counts_as_unexpected() {
    let fake = start_live().await;
    let chat = fake.config().live_chat_id.clone();
    insert(
        &fake,
        json!({ "snippet": { "liveChatId": chat, "type": "pollEvent", "pollDetails": {} } }),
    )
    .await;
    assert_eq!(fake.ledger().unexpected_requests().count(), 1);
}

#[tokio::test]
async fn data_api_without_the_current_token_is_unauthorized() {
    let fake = start_live().await;
    let url = data_api(&fake, "/liveBroadcasts");
    let cases = [
        (None, YouTubeCredentialCheck::MissingBearer),
        (Some("stale-token"), YouTubeCredentialCheck::WrongBearer),
    ];
    for (bearer, expected) in cases {
        let mut request = reqwest::Client::new().get(&url);
        if let Some(token) = bearer {
            request = request.bearer_auth(token);
        }
        let status = request.send().await.unwrap().status().as_u16();
        assert_eq!(
            (status, last_request(&fake).credentials),
            (401, expected),
            "{bearer:?}"
        );
    }
}

#[tokio::test]
async fn active_broadcast_list_follows_the_broadcast_state() {
    let fake = start(FakeYouTubeSetup::default()).await;
    let query = [
        ("part", "snippet,contentDetails"),
        ("broadcastStatus", "active"),
    ];
    let (_, offline) = get(&fake, "/liveBroadcasts", &query).await;
    fake.set_live(true);
    let (_, live) = get(&fake, "/liveBroadcasts", &query).await;

    assert_eq!(
        (
            offline["items"].as_array().map(Vec::len),
            live["items"][0]["snippet"]["liveChatId"].as_str(),
            live["items"][0]["contentDetails"].is_object(),
            live["items"][0].get("status").is_none(),
        ),
        (
            Some(0),
            Some(fake.config().live_chat_id.as_str()),
            true,
            true
        )
    );
}

#[tokio::test]
async fn forge_viewer_poll_reports_the_live_concurrent_viewers() {
    let fake = start(FakeYouTubeSetup {
        live_at_boot: true,
        concurrent_viewers: 77,
        ..FakeYouTubeSetup::default()
    })
    .await;
    let broadcast = ActiveBroadcastIdHandle::new();
    broadcast.set(Some(fake.config().broadcast_id.clone()));
    let (reports_tx, mut reports) = watch::channel(ViewerReport::Absent);
    let poll = YoutubeViewerPoll::new(
        &endpoints(&fake),
        token_source(fake.access_token()),
        broadcast,
        quota(),
        reports_tx,
    );
    let _task = CancelOnDrop(tokio::spawn(poll.run()));

    timeout(WAIT, reports.changed()).await.unwrap().unwrap();

    assert_eq!(*reports.borrow(), ViewerReport::Live { count: 77 });
}

#[tokio::test]
async fn forge_backfills_the_channel_handle_from_its_own_channel() {
    let fake = start(FakeYouTubeSetup::default()).await;
    let manager = manager(&fake, far_future(), None).await;

    let creds = manager.ensure_channel_handle().await.unwrap();

    assert_eq!(
        creds.channel_handle,
        Some(YouTubeAccount::default().channel_handle)
    );
}

#[tokio::test]
async fn channel_list_finds_only_the_seeded_channel() {
    let fake = start(FakeYouTubeSetup::default()).await;
    let account = YouTubeAccount::default();
    let cases = [
        ("mine", "true", 1),
        ("id", account.channel_id.as_str(), 1),
        ("forHandle", "forge_emulator", 1),
        ("forHandle", "@someone_else", 0),
        ("id", "UCsomeoneElse00000000001", 0),
    ];
    for (key, value, expected) in cases {
        let (_, body) = get(&fake, "/channels", &[("part", "snippet"), (key, value)]).await;
        assert_eq!(
            body["items"].as_array().map(Vec::len),
            Some(expected),
            "{key}={value}"
        );
    }
}

fn subscriber(channel_id: &str, private: bool) -> YouTubeSubscriber {
    YouTubeSubscriber {
        channel_id: channel_id.to_owned(),
        subscribed_at: "2023-04-05T06:07:08Z".to_owned(),
        private,
    }
}

#[tokio::test]
async fn subscription_list_answers_from_the_seeded_subscribers_of_this_channel() {
    let public = "UCpublicSubscriber000001";
    let private = "UCprivateSubscriber00001";
    let fake = start(FakeYouTubeSetup {
        subscribers: vec![subscriber(public, false), subscriber(private, true)],
        ..FakeYouTubeSetup::default()
    })
    .await;
    let channel_id = YouTubeAccount::default().channel_id;
    let cases = [
        (
            "public subscriber",
            public,
            channel_id.as_str(),
            200,
            Some(1),
            "",
        ),
        (
            "private subscriber",
            private,
            channel_id.as_str(),
            403,
            None,
            "subscriptionForbidden",
        ),
        (
            "unknown viewer",
            "UCnotSubscribed000000001",
            channel_id.as_str(),
            200,
            Some(0),
            "",
        ),
        (
            "another channel",
            public,
            "UCsomeoneElse00000000001",
            200,
            Some(0),
            "",
        ),
    ];
    for (label, viewer, for_channel, status, items, expected_reason) in cases {
        let (got_status, body) = get(
            &fake,
            "/subscriptions",
            &[
                ("part", "snippet"),
                ("channelId", viewer),
                ("forChannelId", for_channel),
            ],
        )
        .await;

        assert_eq!(
            (
                got_status,
                body["items"].as_array().map(Vec::len),
                reason(&body)
            ),
            (status, items, expected_reason),
            "{label}"
        );
    }
    assert_eq!(fake.ledger().unexpected_requests().count(), 0);
}

#[tokio::test]
async fn subscription_item_carries_the_seeded_date_and_both_channels() {
    let viewer = "UCpublicSubscriber000001";
    let fake = start(FakeYouTubeSetup {
        subscribers: vec![subscriber(viewer, false)],
        ..FakeYouTubeSetup::default()
    })
    .await;
    let channel_id = YouTubeAccount::default().channel_id;

    let (_, body) = get(
        &fake,
        "/subscriptions",
        &[
            ("part", "snippet"),
            ("channelId", viewer),
            ("forChannelId", channel_id.as_str()),
        ],
    )
    .await;

    assert_eq!(
        (
            body.pointer("/items/0/snippet/publishedAt"),
            body.pointer("/items/0/snippet/channelId"),
            body.pointer("/items/0/snippet/resourceId/channelId"),
        ),
        (
            Some(&json!("2023-04-05T06:07:08Z")),
            Some(&json!(viewer)),
            Some(&json!(channel_id)),
        )
    );
}

#[tokio::test]
async fn accepted_refresh_hands_forge_the_token_the_fake_now_accepts() {
    let fake = start(FakeYouTubeSetup::default()).await;
    let manager = manager(&fake, long_expired(), Some("@forge_emulator")).await;

    let served = manager.get_valid_access_token().await.unwrap();

    assert_eq!(
        (
            served.clone(),
            served == YouTubeAccount::default().access_token
        ),
        (fake.access_token(), false)
    );
}

#[tokio::test]
async fn token_retired_by_a_refresh_is_unauthorized() {
    let fake = start_live().await;
    let manager = manager(&fake, long_expired(), Some("@forge_emulator")).await;
    manager.get_valid_access_token().await.unwrap();

    let status = reqwest::Client::new()
        .get(data_api(&fake, "/liveBroadcasts"))
        .bearer_auth(YouTubeAccount::default().access_token)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16();

    assert_eq!(status, 401);
}

#[tokio::test]
async fn refused_refresh_requires_reauthentication() {
    let fake = start(FakeYouTubeSetup {
        refresh: YouTubeRefreshAnswer::Reject,
        ..FakeYouTubeSetup::default()
    })
    .await;
    let manager = manager(&fake, long_expired(), Some("@forge_emulator")).await;

    let refused = manager.get_valid_access_token().await;

    assert!(
        matches!(&refused, Err(PlatformError::ReauthRequired { platform }) if platform == "youtube"),
        "{refused:?}"
    );
}

#[tokio::test]
async fn refresh_ledger_keeps_the_form_but_never_the_client_secret() {
    let fake = start(FakeYouTubeSetup::default()).await;
    let manager = manager(&fake, long_expired(), Some("@forge_emulator")).await;
    manager.get_valid_access_token().await.unwrap();

    let refresh = last_request(&fake);

    assert_eq!(
        (
            refresh.path.as_str(),
            refresh
                .body
                .as_ref()
                .and_then(|body| body.get("client_secret")),
            refresh.body.as_ref().and_then(|body| body.get("client_id")),
        ),
        (TOKEN_PATH, Some(&json!(REDACTED)), Some(&json!(CLIENT_ID)))
    );
}

#[tokio::test]
async fn malformed_refresh_forms_are_refused_with_googles_error_codes() {
    let fake = start(FakeYouTubeSetup::default()).await;
    let url = format!(
        "{}/token",
        endpoints(&fake).base_url(EndpointSurface::YouTubeOAuth)
    );
    let refresh = YouTubeAccount::default().refresh_token;
    let cases = [
        (
            vec![
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
            ],
            "invalid_request",
        ),
        (
            vec![("client_id", CLIENT_ID), ("grant_type", "password")],
            "unsupported_grant_type",
        ),
        (
            vec![
                ("client_id", CLIENT_ID),
                ("grant_type", "refresh_token"),
                ("refresh_token", "spent"),
            ],
            "invalid_grant",
        ),
    ];
    for (form, expected) in cases {
        let response = reqwest::Client::new()
            .post(&url)
            .body(serde_urlencoded::to_string(&form).unwrap())
            .header("content-type", "application/x-www-form-urlencoded")
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body: Value = response.json().await.unwrap();
        assert_eq!(
            (status, body["error"].as_str()),
            (400, Some(expected)),
            "{form:?}"
        );
    }
}

#[tokio::test]
async fn unmodeled_routes_are_recorded_as_unexpected_404s() {
    let fake = start_live().await;
    let overrides = endpoints(&fake);
    let client = reqwest::Client::new();
    let requests = [
        client.put(format!(
            "{}/videos",
            overrides.base_url(EndpointSurface::YouTubeDataApi)
        )),
        client.post(format!(
            "{}/thumbnails/set",
            overrides.base_url(EndpointSurface::YouTubeUploadApi)
        )),
        client.get(format!(
            "{}/revoke",
            overrides.base_url(EndpointSurface::YouTubeOAuth)
        )),
    ];
    for request in requests {
        let status = request
            .bearer_auth(fake.access_token())
            .send()
            .await
            .unwrap()
            .status()
            .as_u16();
        assert_eq!(status, 404);
    }
    assert_eq!(fake.ledger().unexpected_requests().count(), 3);
}

#[tokio::test]
async fn injected_event_keeps_the_given_snippet_and_names_its_author() {
    let fake = start_live().await;
    let mut snippet = Map::new();
    snippet.insert("type".to_owned(), json!("superChatEvent"));
    snippet.insert(
        "superChatDetails".to_owned(),
        json!({ "amountMicros": "2000000", "currency": "EUR" }),
    );
    fake.inject_chat_event(&alice(), snippet).unwrap();

    let (_, page) = list_chat(&fake, None).await;
    let item = &page["items"][0];

    assert_eq!(
        (
            item["snippet"]["superChatDetails"]["amountMicros"].as_str(),
            item["snippet"]["authorChannelId"].as_str(),
            item["snippet"]["liveChatId"].as_str(),
            item["authorDetails"]["displayName"].as_str(),
        ),
        (
            Some("2000000"),
            Some(alice().channel_id.as_str()),
            Some(fake.config().live_chat_id.as_str()),
            Some("Alice"),
        )
    );
}

#[test]
fn debug_output_never_carries_youtube_tokens() {
    let account = YouTubeAccount::default();
    let rendered = [
        format!("{account:?}"),
        format!("{:?}", FakeYouTubeConfig::default()),
    ];
    for text in rendered {
        assert!(
            !text.contains(&account.access_token) && !text.contains(&account.refresh_token),
            "{text}"
        );
    }
}
