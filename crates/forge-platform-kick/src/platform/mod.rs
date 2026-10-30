mod sessions;

use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventChannel, EventSubscription};
use forge_platform_core::{
    ChatPlatform, ConnectionState, PlatformCapabilities, PlatformEndpoints, PlatformError,
};
use forge_storage::BanLedgerRepo;
use tokio::sync::{mpsc, watch};

use crate::ban_ledger::observed_ban;

use crate::api::chat_messages::KickSendChat;
use crate::auth::credentials_manager::KickCredentialsManager;
use crate::capabilities::kick_capabilities;
use crate::platform::sessions::{ChatSessions, PLATFORM_ID, SessionPublisher};
use crate::pusher::chat::{ChatLinkStatus, KickChat};
use crate::pusher::chatroom_lookup::ChannelInfoFetcher;
use crate::pusher::error::KickError;
use crate::vocabulary::event_kinds;

const CHAT_FORWARD_CAPACITY: usize = 256;
pub(crate) const EVENT_CHANNEL_CAPACITY: usize = forge_events::PLATFORM_EVENT_CHANNEL_CAPACITY;

pub struct KickPlatform {
    capabilities: PlatformCapabilities,
    events: Arc<EventChannel>,
    credentials_manager: Arc<KickCredentialsManager>,
    endpoints: PlatformEndpoints,
    channel_info: Arc<ChannelInfoFetcher>,
    sender: Arc<KickSendChat>,
    ban_ledger: Arc<dyn BanLedgerRepo>,
    sessions: ChatSessions,
}

impl KickPlatform {
    pub fn new(
        endpoints: &PlatformEndpoints,
        credentials_manager: Arc<KickCredentialsManager>,
        sender: Arc<KickSendChat>,
        channel_info: Arc<ChannelInfoFetcher>,
        ban_ledger: Arc<dyn BanLedgerRepo>,
    ) -> Self {
        let events = Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY));
        Self {
            capabilities: kick_capabilities(),
            sessions: ChatSessions::new(Arc::clone(&events)),
            events,
            credentials_manager,
            endpoints: endpoints.clone(),
            channel_info,
            sender,
            ban_ledger,
        }
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ConnectionState> {
        self.sessions.state_receiver()
    }

    pub(crate) fn ban_ledger(&self) -> &Arc<dyn BanLedgerRepo> {
        &self.ban_ledger
    }

    async fn send_credentials(&self) -> Result<(String, u64), PlatformError> {
        if !self.capabilities.can_send_chat {
            return Err(PlatformError::Unsupported {
                feature: "chat.send".to_owned(),
            });
        }
        let creds = self.credentials_manager.load().await?.ok_or_else(|| {
            PlatformError::ReauthRequired {
                platform: PLATFORM_ID.to_owned(),
            }
        })?;
        let token = self.credentials_manager.get_valid_access_token().await?;
        Ok((token, creds.user_id))
    }
}

struct ObservedBanRecorder {
    ban_ledger: Arc<dyn BanLedgerRepo>,
    broadcaster_user_id: u64,
}

impl ObservedBanRecorder {
    async fn record(&self, event: &Event) {
        let Some(entry) = observed_ban(&event.payload, self.broadcaster_user_id, event.timestamp)
        else {
            return;
        };
        if let Err(error) = self.ban_ledger.upsert(&entry, event.timestamp).await {
            tracing::warn!(%error, "observed kick ban not recorded in the ban ledger");
        }
    }
}

async fn forward_chat_events(
    mut chat_rx: mpsc::Receiver<Event>,
    recorder: ObservedBanRecorder,
    publisher: SessionPublisher,
) {
    while let Some(event) = chat_rx.recv().await {
        if event.kind == event_kinds::MODERATION_BANNED {
            recorder.record(&event).await;
        }
        if !publisher.publish_event(event) {
            break;
        }
    }
}

async fn forward_link_status(
    mut state_rx: watch::Receiver<ChatLinkStatus>,
    publisher: SessionPublisher,
) {
    loop {
        let status = *state_rx.borrow_and_update();
        if !publisher.publish_status(status) || state_rx.changed().await.is_err() {
            break;
        }
    }
}

#[async_trait]
impl ChatPlatform for KickPlatform {
    fn connection_state(&self) -> ConnectionState {
        self.sessions.connection_state()
    }

    async fn connect(&self) -> Result<(), PlatformError> {
        let creds = self.credentials_manager.load().await?.ok_or_else(|| {
            PlatformError::ReauthRequired {
                platform: PLATFORM_ID.to_owned(),
            }
        })?;

        let ticket = self.sessions.begin().await;
        let (chat_tx, chat_rx) = mpsc::channel::<Event>(CHAT_FORWARD_CAPACITY);
        let connected = KickChat::new(
            &self.endpoints,
            creds.username,
            Arc::clone(&self.channel_info),
        )
        .connect_until(chat_tx, ticket.cancellation())
        .await;
        let mut handle = match connected {
            Ok(handle) => handle,
            Err(KickError::ConnectCancelled) => return Ok(()),
            Err(err) => {
                self.sessions.abandon(&ticket);
                return Err(map_connect_error(err));
            }
        };

        let publisher = self.sessions.publisher(&ticket);
        handle.attach(forward_chat_events(
            chat_rx,
            ObservedBanRecorder {
                ban_ledger: Arc::clone(&self.ban_ledger),
                broadcaster_user_id: creds.user_id,
            },
            publisher.clone(),
        ));
        handle.attach(forward_link_status(handle.state_receiver(), publisher));

        self.sessions.install(ticket, handle).await;
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), PlatformError> {
        self.sessions.end().await;
        Ok(())
    }

    async fn send_message(&self, _channel: &str, text: &str) -> Result<(), PlatformError> {
        let (token, broadcaster_user_id) = self.send_credentials().await?;
        self.sender
            .send(text, &token, broadcaster_user_id, false)
            .await
    }

    async fn send_reply(
        &self,
        _channel: &str,
        reply_parent_message_id: &str,
        text: &str,
    ) -> Result<(), PlatformError> {
        let (token, broadcaster_user_id) = self.send_credentials().await?;
        self.sender
            .send_reply(text, &token, broadcaster_user_id, reply_parent_message_id)
            .await
    }

    fn events(&self) -> EventSubscription {
        self.events.subscribe()
    }
}

fn map_connect_error(err: KickError) -> PlatformError {
    match err {
        KickError::Http { status, body } => PlatformError::Http { status, body },
        KickError::Network { reason }
        | KickError::WebSocket { reason }
        | KickError::ChannelInfoUnavailable { reason, .. } => PlatformError::Network { reason },
        KickError::ChatroomIdNotFound { slug } => PlatformError::Network {
            reason: format!("chatroom_id missing for '{slug}'"),
        },
        KickError::ConnectCancelled => PlatformError::Network {
            reason: KickError::ConnectCancelled.to_string(),
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use forge_types::OAuthToken;
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration as StdDuration;

    use forge_events::{PlatformConnectionChanged, TypedEvent};
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use time::{Duration, OffsetDateTime};

    use super::*;
    use crate::auth::credentials::{CREDENTIAL_KEY, KickCredentials};
    use crate::pusher::chatroom_lookup::ChannelInfoFetcher;

    struct InMemRepo(StdMutex<HashMap<String, String>>);

    impl InMemRepo {
        fn empty() -> Arc<Self> {
            Arc::new(Self(StdMutex::new(HashMap::new())))
        }

        fn with_valid_creds() -> Arc<Self> {
            let creds = KickCredentials {
                access_token: OAuthToken::new("tok"),
                refresh_token: OAuthToken::new("ref"),
                user_id: 42,
                username: "streamer".to_owned(),
                client_id: "cid".to_owned(),
                expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
            };
            let mut map = HashMap::new();
            map.insert(
                CREDENTIAL_KEY.to_owned(),
                serde_json::to_string(&creds).unwrap(),
            );
            Arc::new(Self(StdMutex::new(map)))
        }
    }

    #[async_trait]
    impl CredentialsRepo for InMemRepo {
        async fn store(&self, id: &CredentialId, v: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), v.to_owned());
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
        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }
        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn platform(repo: Arc<InMemRepo>, api: forge_net::Gateway) -> KickPlatform {
        let manager = Arc::new(crate::endpoints_test_support::offline_manager(repo));
        KickPlatform::new(
            &crate::endpoints_test_support::unreachable_endpoints(),
            manager,
            Arc::new(KickSendChat::new(api)),
            crate::endpoints_test_support::unreachable_channel_info(),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
    }

    #[test]
    fn connection_state_is_disconnected_before_connect() {
        let p = platform(
            InMemRepo::empty(),
            crate::endpoints_test_support::unreachable_api(),
        );
        assert_eq!(p.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn connect_without_credentials_requires_reauth_and_stays_disconnected() {
        let p = platform(
            InMemRepo::empty(),
            crate::endpoints_test_support::unreachable_api(),
        );
        let err = p.connect().await.unwrap_err();
        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "kick"),
            "expected ReauthRequired {{ platform: kick }}, got {err:?}"
        );
        assert_eq!(p.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn send_message_without_credentials_requires_reauth() {
        let p = platform(
            InMemRepo::empty(),
            crate::endpoints_test_support::unreachable_api(),
        );
        let err = p.send_message("chan", "hello").await.unwrap_err();
        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "kick"),
            "expected ReauthRequired {{ platform: kick }}, got {err:?}"
        );
    }

    #[tokio::test]
    async fn send_message_with_valid_credentials_delegates_to_rate_limited_sender() {
        let p = platform(
            InMemRepo::with_valid_creds(),
            crate::endpoints_test_support::api_limited_by(
                "http://127.0.0.1:0",
                Arc::new(crate::endpoints_test_support::ExhaustedLimiter),
            ),
        );
        let err = p.send_message("chan", "hello").await.unwrap_err();
        assert!(matches!(err, PlatformError::RateLimitExhausted));
    }

    #[tokio::test]
    async fn send_reply_posts_the_text_with_the_parent_message_id() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let p = platform(
            InMemRepo::with_valid_creds(),
            crate::endpoints_test_support::api_at(&server.uri()),
        );

        p.send_reply("chan", "parent-7", "hello").await.unwrap();

        let reqs = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["content"], "hello");
        assert_eq!(body["reply_to_message_id"], "parent-7");
    }

    const BANNED_STEP_DEADLINE: StdDuration = StdDuration::from_secs(30);

    #[derive(Default)]
    struct GatedBanLedger {
        upserts: StdMutex<Vec<forge_storage::BanLedgerEntry>>,
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    #[async_trait]
    impl BanLedgerRepo for GatedBanLedger {
        async fn upsert(
            &self,
            entry: &forge_storage::BanLedgerEntry,
            _now: OffsetDateTime,
        ) -> Result<forge_storage::BanLedgerEntry, StorageError> {
            self.upserts.lock().unwrap().push(entry.clone());
            self.entered.notify_one();
            self.release.notified().await;
            Ok(entry.clone())
        }

        async fn remove(&self, _key: &forge_storage::BanLedgerKey) -> Result<bool, StorageError> {
            Ok(false)
        }

        async fn get(
            &self,
            _key: &forge_storage::BanLedgerKey,
            _now: OffsetDateTime,
        ) -> Result<Option<forge_storage::BanLedgerEntry>, StorageError> {
            Ok(None)
        }

        async fn list_active(
            &self,
            _platform: forge_types::PlatformId,
            _channel_id: &str,
            _now: OffsetDateTime,
            _limit: usize,
        ) -> Result<Vec<forge_storage::BanLedgerEntry>, StorageError> {
            Ok(Vec::new())
        }
    }

    fn user_banned_frame(duration_secs: Option<u64>) -> String {
        let mut inner = serde_json::json!({
            "user": { "id": 77, "username": "Troll" },
            "banned_by": { "id": 2, "username": "ModAlice" },
            "permanent_ban_reason": ""
        });
        if let Some(seconds) = duration_secs {
            inner["duration"] = serde_json::json!(seconds);
        }
        serde_json::json!({
            "event": "App\\Events\\UserBannedEvent",
            "channel": "chatrooms.4242.v2",
            "data": inner.to_string()
        })
        .to_string()
    }

    fn subscription_ack() -> String {
        serde_json::json!({
            "event": crate::pusher::protocol::SUBSCRIPTION_SUCCEEDED,
            "channel": "chatrooms.4242.v2",
            "data": "{}"
        })
        .to_string()
    }

    async fn pusher_peer_sending(frame: String) -> String {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(message)) = ws.next().await {
                if matches!(message, Message::Text(_)) {
                    break;
                }
            }
            ws.send(Message::Text(subscription_ack().into()))
                .await
                .unwrap();
            ws.send(Message::Text(frame.into())).await.unwrap();
            while let Some(Ok(_)) = ws.next().await {}
        });
        format!("ws://{socket_addr}/app")
    }

    pub(crate) async fn platform_on_chat(
        frame: String,
        ledger: Arc<dyn BanLedgerRepo>,
    ) -> (KickPlatform, wiremock::MockServer) {
        let socket_base = pusher_peer_sending(frame).await;
        platform_on_socket(socket_base, ledger).await
    }

    async fn platform_on_socket(
        socket_base: String,
        ledger: Arc<dyn BanLedgerRepo>,
    ) -> (KickPlatform, wiremock::MockServer) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v2/channels/streamer"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "chatroom": { "id": 4242 },
                "livestream": null
            })))
            .mount(&server)
            .await;
        let platform = platform_on(socket_base, format!("{}/api/v2", server.uri()), ledger);
        (platform, server)
    }

    fn endpoints_on(socket_base: String, channel_api_base: String) -> PlatformEndpoints {
        use forge_platform_core::EndpointSurface;

        let overrides = [
            (EndpointSurface::KickChannelApi.env_var(), channel_api_base),
            (EndpointSurface::KickChatSocket.env_var(), socket_base),
        ];
        PlatformEndpoints::resolve(|variable| {
            overrides
                .iter()
                .find(|(name, _)| *name == variable)
                .map(|(_, base)| std::ffi::OsString::from(base))
        })
        .unwrap()
    }

    fn platform_on(
        socket_base: String,
        channel_api_base: String,
        ledger: Arc<dyn BanLedgerRepo>,
    ) -> KickPlatform {
        let endpoints = endpoints_on(socket_base, channel_api_base);
        let channel_info = ChannelInfoFetcher::new(
            crate::gateway::kick_channel_info_gateway(
                &endpoints,
                Arc::new(crate::endpoints_test_support::GrantingLimiter),
                Arc::new(crate::endpoints_test_support::RecordingPublisher::default()),
            )
            .unwrap(),
        );
        platform_sharing(&endpoints, Arc::new(channel_info), ledger)
    }

    fn platform_sharing(
        endpoints: &PlatformEndpoints,
        channel_info: Arc<ChannelInfoFetcher>,
        ledger: Arc<dyn BanLedgerRepo>,
    ) -> KickPlatform {
        let manager = Arc::new(KickCredentialsManager::new(
            InMemRepo::with_valid_creds(),
            crate::endpoints_test_support::oauth_client_over(
                endpoints,
                Arc::new(crate::endpoints_test_support::GrantingLimiter),
            ),
            crate::endpoints_test_support::unreachable_api(),
        ));
        KickPlatform::new(
            endpoints,
            manager,
            Arc::new(KickSendChat::new(
                crate::endpoints_test_support::unreachable_api(),
            )),
            channel_info,
            ledger,
        )
    }

    async fn next_banned_event(stream: &mut EventSubscription) -> Event {
        tokio::time::timeout(BANNED_STEP_DEADLINE, async {
            loop {
                let event = stream.recv().await.unwrap();
                if event.kind == event_kinds::MODERATION_BANNED {
                    return event;
                }
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn observed_ban_is_recorded_before_the_event_is_published() {
        use futures::FutureExt;

        let ledger = Arc::new(GatedBanLedger::default());
        let (platform, _server) = platform_on_chat(
            user_banned_frame(None),
            Arc::clone(&ledger) as Arc<dyn BanLedgerRepo>,
        )
        .await;
        let mut stream = platform.events();
        platform.connect().await.unwrap();

        tokio::time::timeout(BANNED_STEP_DEADLINE, ledger.entered.notified())
            .await
            .unwrap();
        let mut published_while_recording = Vec::new();
        while let Some(Ok(event)) = stream.recv().now_or_never() {
            published_while_recording.push(event.kind);
        }
        ledger.release.notify_one();

        assert!(
            !published_while_recording
                .iter()
                .any(|kind| kind == event_kinds::MODERATION_BANNED),
            "published before the ledger write finished: {published_while_recording:?}"
        );
        next_banned_event(&mut stream).await;
        platform.disconnect().await.unwrap();
    }

    #[tokio::test]
    async fn observed_ban_is_recorded_under_the_connected_broadcaster_with_its_term() {
        for (duration_secs, expected_term) in
            [(None, None), (Some(600), Some(Duration::minutes(10)))]
        {
            let ledger = Arc::new(crate::ban_ledger_test_support::MemoryBanLedger::default());
            let (platform, _server) = platform_on_chat(
                user_banned_frame(duration_secs),
                Arc::clone(&ledger) as Arc<dyn BanLedgerRepo>,
            )
            .await;
            let mut stream = platform.events();
            platform.connect().await.unwrap();

            next_banned_event(&mut stream).await;
            platform.disconnect().await.unwrap();

            let stored = ledger
                .stored(&forge_storage::BanLedgerKey {
                    platform: forge_types::PlatformId::Kick,
                    channel_id: "42".to_owned(),
                    viewer_id: "77".to_owned(),
                })
                .unwrap();
            assert_eq!(
                (
                    stored.origin,
                    stored
                        .expires_at
                        .map(|expires_at| expires_at - stored.banned_at),
                ),
                (forge_storage::BanOrigin::Observed, expected_term),
                "duration {duration_secs:?}"
            );
        }
    }

    #[tokio::test]
    async fn observed_ban_is_published_when_the_ledger_write_fails() {
        let (platform, _server) = platform_on_chat(
            user_banned_frame(None),
            Arc::new(crate::ban_ledger_test_support::MemoryBanLedger::failing_on(
                crate::ban_ledger_test_support::LedgerOp::Upsert,
            )),
        )
        .await;
        let mut stream = platform.events();
        platform.connect().await.unwrap();

        let event = next_banned_event(&mut stream).await;
        platform.disconnect().await.unwrap();

        assert_eq!(event.payload["banned_user"]["id"], 77);
    }

    #[derive(Debug, Clone, Copy)]
    enum PeerEnding {
        Holds,
        ClosesWith(u16),
        DropsTheSocket,
    }

    async fn pusher_peer_ending(ending: PeerEnding) -> String {
        let (socket_base, release) = pusher_peer_ending_on_release(ending).await;
        release.notify_one();
        socket_base
    }

    async fn pusher_peer_ending_on_release(
        ending: PeerEnding,
    ) -> (String, Arc<tokio::sync::Notify>) {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;
        use tokio_tungstenite::tungstenite::protocol::CloseFrame;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket_addr = listener.local_addr().unwrap();
        let release = Arc::new(tokio::sync::Notify::new());
        let released = Arc::clone(&release);
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(message)) = ws.next().await {
                if matches!(message, Message::Text(_)) {
                    break;
                }
            }
            ws.send(Message::Text(subscription_ack().into()))
                .await
                .unwrap();
            match ending {
                PeerEnding::Holds => {}
                PeerEnding::ClosesWith(code) => {
                    released.notified().await;
                    let frame = CloseFrame {
                        code: code.into(),
                        reason: "closed by the test peer".into(),
                    };
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the receiver may already be gone"
                    )]
                    let _ = ws.send(Message::Close(Some(frame))).await;
                }
                PeerEnding::DropsTheSocket => {
                    released.notified().await;
                    return;
                }
            }
            while let Some(Ok(_)) = ws.next().await {}
        });
        (format!("ws://{socket_addr}/app"), release)
    }

    fn link_announcement(event: &Event) -> Option<(String, Option<bool>)> {
        (event.kind == PlatformConnectionChanged::KIND).then(|| {
            (
                event.payload["state"].as_str().unwrap().to_owned(),
                event.payload["expected"].as_bool(),
            )
        })
    }

    async fn next_link_announcement(stream: &mut EventSubscription) -> (String, Option<bool>) {
        tokio::time::timeout(BANNED_STEP_DEADLINE, async {
            loop {
                if let Some(announcement) = link_announcement(&stream.recv().await.unwrap()) {
                    return announcement;
                }
            }
        })
        .await
        .unwrap()
    }

    async fn await_link_state(stream: &mut EventSubscription, state: &str) {
        while next_link_announcement(stream).await.0 != state {}
    }

    fn published_link_announcements(stream: &mut EventSubscription) -> Vec<(String, Option<bool>)> {
        use futures::FutureExt;

        let mut announcements = Vec::new();
        while let Some(Ok(event)) = stream.recv().now_or_never() {
            announcements.extend(link_announcement(&event));
        }
        announcements
    }

    const RETIRED_LOOP_WINDOW: StdDuration = StdDuration::from_secs(2);
    const NO_PROGRESS_HORIZON: StdDuration = StdDuration::from_secs(3600);

    fn channel_found() -> wiremock::ResponseTemplate {
        wiremock::ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(serde_json::json!({
            "chatroom": { "id": 4242 },
            "livestream": null
        }))
    }

    async fn pusher_peer_dropping_only_the_first_link() -> String {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut first_link = true;
            while let Ok((stream, _)) = listener.accept().await {
                let drops_after_subscribe = std::mem::replace(&mut first_link, false);
                tokio::spawn(async move {
                    let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    while let Some(Ok(message)) = ws.next().await {
                        if matches!(message, Message::Text(_)) {
                            break;
                        }
                    }
                    ws.send(Message::Text(subscription_ack().into()))
                        .await
                        .unwrap();
                    if drops_after_subscribe {
                        return;
                    }
                    while let Some(Ok(_)) = ws.next().await {}
                });
            }
        });
        format!("ws://{socket_addr}/app")
    }

    async fn channel_api_that_never_answers() -> (String, Arc<tokio::sync::Notify>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api_addr = listener.local_addr().unwrap();
        let requested = Arc::new(tokio::sync::Notify::new());
        let requested_signal = Arc::clone(&requested);
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
                requested_signal.notify_one();
            }
        });
        (format!("http://{api_addr}"), requested)
    }

    async fn platform_with_a_connect_in_flight() -> (
        Arc<KickPlatform>,
        tokio::task::JoinHandle<Result<(), PlatformError>>,
    ) {
        let (api_base, requested) = channel_api_that_never_answers().await;
        let platform = Arc::new(platform_on(
            pusher_peer_ending(PeerEnding::Holds).await,
            api_base,
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        ));
        let connecting = Arc::clone(&platform);
        let in_flight = tokio::spawn(async move { connecting.connect().await });
        tokio::time::timeout(BANNED_STEP_DEADLINE, requested.notified())
            .await
            .unwrap();
        (platform, in_flight)
    }

    #[tokio::test]
    async fn reconnecting_while_the_retired_loop_backs_off_leaves_connected_as_the_last_announcement()
     {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let channel_api = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(channel_found())
            .up_to_n_times(1)
            .mount(&channel_api)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(
                reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            ))
            .mount(&channel_api)
            .await;
        let platform = platform_on(
            pusher_peer_dropping_only_the_first_link().await,
            channel_api.uri(),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        );
        let mut stream = platform.events();
        platform.connect().await.unwrap();
        await_link_state(&mut stream, "reconnecting").await;

        Mock::given(method("GET"))
            .respond_with(channel_found())
            .with_priority(1)
            .mount(&channel_api)
            .await;
        platform.connect().await.unwrap();
        published_link_announcements(&mut stream);
        await_link_state(&mut stream, "connected").await;
        tokio::time::sleep(RETIRED_LOOP_WINDOW).await;
        let late = published_link_announcements(&mut stream);
        platform.disconnect().await.unwrap();

        assert!(
            late.is_empty(),
            "announced after the new session connected: {late:?}"
        );
    }

    #[tokio::test]
    async fn disconnect_during_reconnect_backoff_settles_disconnected_expected_once() {
        let (platform, _server) = platform_on_socket(
            pusher_peer_ending(PeerEnding::DropsTheSocket).await,
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
        .await;
        let mut stream = platform.events();
        platform.connect().await.unwrap();
        await_link_state(&mut stream, "reconnecting").await;

        platform.disconnect().await.unwrap();

        let settled: Vec<_> = published_link_announcements(&mut stream)
            .into_iter()
            .filter(|(state, _)| state != "reconnecting")
            .collect();
        assert_eq!(settled, vec![("disconnected".to_owned(), Some(true))]);
    }

    #[tokio::test]
    async fn disconnect_during_an_in_flight_connect_resolves_that_connect_ok_and_disconnected() {
        let (platform, in_flight) = platform_with_a_connect_in_flight().await;

        platform.disconnect().await.unwrap();
        let resolved = tokio::time::timeout(NO_PROGRESS_HORIZON, in_flight).await;

        assert_eq!(
            (
                matches!(resolved, Ok(Ok(Ok(())))),
                platform.connection_state()
            ),
            (true, ConnectionState::Disconnected)
        );
    }

    #[tokio::test]
    async fn a_connect_superseded_by_a_newer_connect_resolves_ok() {
        let (platform, superseded) = platform_with_a_connect_in_flight().await;

        let newer = Arc::clone(&platform);
        let _newer = tokio::spawn(async move { newer.connect().await });
        let resolved = tokio::time::timeout(NO_PROGRESS_HORIZON, superseded).await;
        platform.disconnect().await.unwrap();

        assert!(matches!(resolved, Ok(Ok(Ok(())))), "{resolved:?}");
    }

    async fn pusher_peer_that_never_acknowledges() -> (String, Arc<tokio::sync::Notify>) {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket_addr = listener.local_addr().unwrap();
        let subscribed = Arc::new(tokio::sync::Notify::new());
        let signal = Arc::clone(&subscribed);
        tokio::spawn(async move {
            let mut links = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                let signal = Arc::clone(&signal);
                links.push(tokio::spawn(async move {
                    let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    while let Some(Ok(message)) = ws.next().await {
                        if matches!(message, Message::Text(_)) {
                            signal.notify_one();
                            break;
                        }
                    }
                    while let Some(Ok(_)) = ws.next().await {}
                }));
            }
        });
        (format!("ws://{socket_addr}/app"), subscribed)
    }

    #[tokio::test]
    async fn the_link_stays_connecting_until_the_subscription_is_acknowledged() {
        let (socket_base, subscribed) = pusher_peer_that_never_acknowledges().await;
        let (platform, _server) = platform_on_socket(
            socket_base,
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
        .await;
        platform.connect().await.unwrap();
        tokio::time::timeout(BANNED_STEP_DEADLINE, subscribed.notified())
            .await
            .unwrap();

        let state = platform.connection_state();
        platform.disconnect().await.unwrap();

        assert_eq!(state, ConnectionState::Connecting);
    }

    #[tokio::test]
    async fn an_unacknowledged_subscription_reconnects_without_ever_announcing_connected() {
        let (socket_base, subscribed) = pusher_peer_that_never_acknowledges().await;
        let (platform, _server) = platform_on_socket(
            socket_base,
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
        .await;
        let mut stream = platform.events();
        platform.connect().await.unwrap();
        tokio::time::timeout(BANNED_STEP_DEADLINE, subscribed.notified())
            .await
            .unwrap();

        let mut announced = Vec::new();
        loop {
            let (state, _) = next_link_announcement(&mut stream).await;
            let reconnecting = state == "reconnecting";
            announced.push(state);
            if reconnecting {
                break;
            }
        }
        platform.disconnect().await.unwrap();

        assert_eq!(announced, ["connecting", "reconnecting"]);
    }

    #[tokio::test]
    async fn platform_state_is_readable_by_a_receiver_subscribed_after_the_change() {
        let (platform, _server) = platform_on_socket(
            pusher_peer_ending(PeerEnding::Holds).await,
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
        .await;
        let mut stream = platform.events();
        platform.connect().await.unwrap();
        await_link_state(&mut stream, "connected").await;
        let after_connect = *platform.state_receiver().borrow();

        platform.disconnect().await.unwrap();
        let after_disconnect = *platform.state_receiver().borrow();

        assert_eq!(
            (after_connect, after_disconnect),
            (ConnectionState::Connected, ConnectionState::Disconnected)
        );
    }

    #[tokio::test]
    async fn leaving_connected_is_flagged_expected_only_for_a_requested_close_or_a_pusher_reconnect_code()
     {
        for (ending, want_state, want_expected) in [
            (PeerEnding::Holds, "disconnected", true),
            (PeerEnding::ClosesWith(4200), "reconnecting", true),
            (PeerEnding::ClosesWith(4299), "reconnecting", true),
            (PeerEnding::ClosesWith(4199), "reconnecting", false),
            (PeerEnding::ClosesWith(4300), "reconnecting", false),
            (PeerEnding::ClosesWith(1000), "reconnecting", false),
            (PeerEnding::DropsTheSocket, "reconnecting", false),
        ] {
            let (socket_base, release) = pusher_peer_ending_on_release(ending).await;
            let (platform, _server) = platform_on_socket(
                socket_base,
                crate::ban_ledger_test_support::MemoryBanLedger::shared(),
            )
            .await;
            let mut stream = platform.events();
            platform.connect().await.unwrap();
            await_link_state(&mut stream, "connected").await;
            release.notify_one();

            if matches!(ending, PeerEnding::Holds) {
                platform.disconnect().await.unwrap();
            }

            assert_eq!(
                next_link_announcement(&mut stream).await,
                (want_state.to_owned(), Some(want_expected)),
                "{ending:?}"
            );
            platform.disconnect().await.unwrap();
        }
    }

    async fn primed_channel_info() -> (Arc<ChannelInfoFetcher>, wiremock::MockServer) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/streamer"))
            .respond_with(channel_found())
            .mount(&server)
            .await;
        let fetcher = Arc::new(crate::endpoints_test_support::channel_info_at(
            &server.uri(),
        ));
        fetcher.fetch("streamer").await.unwrap();
        (fetcher, server)
    }

    async fn pusher_peer_answering_only_the_first_link()
    -> (String, tokio::sync::mpsc::UnboundedReceiver<usize>) {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket_addr = listener.local_addr().unwrap();
        let (accepted_tx, accepted_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut held = Vec::new();
            let mut accepted = 0_usize;
            while let Ok((stream, _)) = listener.accept().await {
                accepted += 1;
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the receiver may already be gone"
                )]
                let _ = accepted_tx.send(accepted);
                if accepted > 1 {
                    held.push(stream);
                    continue;
                }
                tokio::spawn(async move {
                    let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    while let Some(Ok(message)) = ws.next().await {
                        if matches!(message, Message::Text(_)) {
                            break;
                        }
                    }
                    ws.send(Message::Text(subscription_ack().into()))
                        .await
                        .unwrap();
                });
            }
        });
        (format!("ws://{socket_addr}/app"), accepted_rx)
    }

    async fn silent_pusher_socket() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let socket_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
            }
        });
        format!("ws://{socket_addr}/app")
    }

    const PUSHER_CONNECT_BOUND: StdDuration = StdDuration::from_secs(10);

    #[tokio::test]
    async fn a_first_connect_to_a_silent_pusher_socket_fails_as_network_after_the_connect_bound() {
        let (channel_info, server) = primed_channel_info().await;
        let endpoints = endpoints_on(silent_pusher_socket().await, server.uri());
        let platform = platform_sharing(
            &endpoints,
            channel_info,
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        );
        let started = tokio::time::Instant::now();

        let result = platform.connect().await;
        let waited = started.elapsed();

        assert!(
            matches!(&result, Err(PlatformError::Network { reason }) if reason.contains("timed out after 10s")),
            "{result:?}"
        );
        assert!(waited >= PUSHER_CONNECT_BOUND, "waited {waited:?}");
        assert_eq!(platform.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn a_reconnect_that_times_out_goes_back_to_backoff_and_dials_again() {
        let (channel_info, server) = primed_channel_info().await;
        let (socket_base, mut accepted) = pusher_peer_answering_only_the_first_link().await;
        let endpoints = endpoints_on(socket_base, server.uri());
        let platform = platform_sharing(
            &endpoints,
            channel_info,
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        );
        platform.connect().await.unwrap();
        let first_redial = tokio::time::timeout(BANNED_STEP_DEADLINE, async {
            while accepted.recv().await.unwrap() < 2 {}
        })
        .await;

        let second_redial = tokio::time::timeout(BANNED_STEP_DEADLINE, accepted.recv())
            .await
            .ok()
            .flatten();
        let state = platform.connection_state();
        platform.disconnect().await.unwrap();

        assert!(first_redial.is_ok(), "the lost link was never redialled");
        assert_eq!(
            (second_redial, state),
            (Some(3), ConnectionState::Reconnecting)
        );
    }
}
