use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventStream};
use forge_platform_core::{
    ChatPlatform, ConnectionState, PlatformCapabilities, PlatformEndpoints, PlatformError,
    RateLimiter, connection_state_changed_event,
};
use forge_storage::BanLedgerRepo;
use tokio::sync::{mpsc, watch};

use crate::ban_ledger::{USER_BANNED_EVENT_KIND, observed_ban};

use crate::capabilities::kick_capabilities;
use crate::chat::{KickChat, KickChatHandle};
use crate::credentials_manager::KickCredentialsManager;
use crate::error::KickError;
use crate::event_channel::PlatformEventChannel;
use crate::send::KickSendChat;

const PLATFORM_ID: &str = "kick";
const CHAT_FORWARD_CAPACITY: usize = 256;

pub struct KickPlatform {
    capabilities: PlatformCapabilities,
    events: Arc<PlatformEventChannel>,
    credentials_manager: Arc<KickCredentialsManager>,
    endpoints: PlatformEndpoints,
    http: reqwest::Client,
    sender: KickSendChat,
    ban_ledger: Arc<dyn BanLedgerRepo>,
    handle: Mutex<Option<KickChatHandle>>,
    state_tx: watch::Sender<ConnectionState>,
}

impl KickPlatform {
    pub fn new(
        endpoints: &PlatformEndpoints,
        credentials_manager: Arc<KickCredentialsManager>,
        rate_limiter: Arc<dyn RateLimiter>,
        ban_ledger: Arc<dyn BanLedgerRepo>,
    ) -> Self {
        let (state_tx, _) = watch::channel(ConnectionState::Disconnected);
        Self {
            capabilities: kick_capabilities(),
            events: Arc::new(PlatformEventChannel::new()),
            credentials_manager,
            endpoints: endpoints.clone(),
            http: reqwest::Client::new(),
            sender: KickSendChat::new(endpoints, rate_limiter),
            ban_ledger,
            handle: Mutex::new(None),
            state_tx,
        }
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ConnectionState> {
        self.state_tx.subscribe()
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

async fn record_observed_ban(
    ban_ledger: &dyn BanLedgerRepo,
    broadcaster_user_id: u64,
    event: &Event,
) {
    let Some(entry) = observed_ban(&event.payload, broadcaster_user_id, event.timestamp) else {
        return;
    };
    if let Err(error) = ban_ledger.upsert(&entry, event.timestamp).await {
        tracing::warn!(%error, "observed kick ban not recorded in the ban ledger");
    }
}

#[async_trait]
impl ChatPlatform for KickPlatform {
    fn connection_state(&self) -> ConnectionState {
        self.handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(KickChatHandle::connection_state)
            .unwrap_or(ConnectionState::Disconnected)
    }

    async fn connect(&self) -> Result<(), PlatformError> {
        let creds = self.credentials_manager.load().await?.ok_or_else(|| {
            PlatformError::ReauthRequired {
                platform: PLATFORM_ID.to_owned(),
            }
        })?;

        let previous = self.handle.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(previous) = previous {
            previous.shutdown();
        }

        let (chat_tx, mut chat_rx) = mpsc::channel::<Event>(CHAT_FORWARD_CAPACITY);
        let handle = KickChat::new(&self.endpoints, creds.username, self.http.clone())
            .connect(chat_tx)
            .await
            .map_err(map_connect_error)?;

        let forward_events = self.events.clone();
        let ban_ledger = Arc::clone(&self.ban_ledger);
        let broadcaster_user_id = creds.user_id;
        tokio::spawn(async move {
            while let Some(event) = chat_rx.recv().await {
                if event.kind == USER_BANNED_EVENT_KIND {
                    record_observed_ban(ban_ledger.as_ref(), broadcaster_user_id, &event).await;
                }
                forward_events.publish(event);
            }
        });

        let state_events = self.events.clone();
        let platform_state_tx = self.state_tx.clone();
        let mut state_rx = handle.state_receiver();
        tokio::spawn(async move {
            loop {
                let state = *state_rx.borrow_and_update();
                state_events.publish(connection_state_changed_event(PLATFORM_ID, state));
                let _ = platform_state_tx.send(state);
                if state_rx.changed().await.is_err() {
                    break;
                }
            }
        });

        *self.handle.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), PlatformError> {
        let handle = self.handle.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(handle) = handle {
            handle.shutdown();
        }
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

    fn events(&self) -> EventStream {
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
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;
    use std::time::Duration as StdDuration;

    use forge_platform_core::{RateLimitOutcome, RateLimiter};
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use time::{Duration, OffsetDateTime};

    use super::*;
    use crate::credentials::{CREDENTIAL_KEY, KickCredentials};

    struct InMemRepo(StdMutex<HashMap<String, String>>);

    impl InMemRepo {
        fn empty() -> Arc<Self> {
            Arc::new(Self(StdMutex::new(HashMap::new())))
        }

        fn with_valid_creds() -> Arc<Self> {
            let creds = KickCredentials {
                access_token: "tok".to_owned(),
                refresh_token: "ref".to_owned(),
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

    struct GrantLimiter;
    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }
        async fn observe_remote_throttle(&self, _retry_after: StdDuration) {}
    }

    struct ExhaustedLimiter;
    #[async_trait]
    impl RateLimiter for ExhaustedLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Exhausted)
        }
        async fn observe_remote_throttle(&self, _retry_after: StdDuration) {}
    }

    fn platform(repo: Arc<InMemRepo>, limiter: Arc<dyn RateLimiter>) -> KickPlatform {
        let manager = Arc::new(KickCredentialsManager::new(
            &forge_platform_core::PlatformEndpoints::default(),
            repo,
            "test_cid".to_owned(),
            "test_secret".to_owned(),
        ));
        KickPlatform::new(
            &forge_platform_core::PlatformEndpoints::default(),
            manager,
            limiter,
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        )
    }

    #[test]
    fn connection_state_is_disconnected_before_connect() {
        let p = platform(InMemRepo::empty(), Arc::new(GrantLimiter));
        assert_eq!(p.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn connect_without_credentials_requires_reauth_and_stays_disconnected() {
        let p = platform(InMemRepo::empty(), Arc::new(GrantLimiter));
        let err = p.connect().await.unwrap_err();
        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "kick"),
            "expected ReauthRequired {{ platform: kick }}, got {err:?}"
        );
        assert_eq!(p.connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn send_message_without_credentials_requires_reauth() {
        let p = platform(InMemRepo::empty(), Arc::new(GrantLimiter));
        let err = p.send_message("chan", "hello").await.unwrap_err();
        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "kick"),
            "expected ReauthRequired {{ platform: kick }}, got {err:?}"
        );
    }

    #[tokio::test]
    async fn send_message_with_valid_credentials_delegates_to_rate_limited_sender() {
        let p = platform(InMemRepo::with_valid_creds(), Arc::new(ExhaustedLimiter));
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
        let mut p = platform(InMemRepo::with_valid_creds(), Arc::new(GrantLimiter));
        p.sender = KickSendChat::new(
            &forge_platform_core::PlatformEndpoints::default(),
            Arc::new(GrantLimiter),
        )
        .with_send_endpoint(server.uri());

        p.send_reply("chan", "parent-7", "hello").await.unwrap();

        let reqs = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(body["content"], "hello");
        assert_eq!(body["reply_to_message_id"], "parent-7");
    }

    const BANNED_STEP_DEADLINE: StdDuration = StdDuration::from_secs(5);

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
            _platform: forge_storage::ViewerPlatform,
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
            ws.send(Message::Text(frame.into())).await.unwrap();
            while let Some(Ok(_)) = ws.next().await {}
        });
        format!("ws://{socket_addr}/app")
    }

    pub(crate) async fn platform_on_chat(
        frame: String,
        ledger: Arc<dyn BanLedgerRepo>,
    ) -> (KickPlatform, wiremock::MockServer) {
        use forge_platform_core::EndpointSurface;
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
        let socket_base = pusher_peer_sending(frame).await;
        let overrides = [
            (
                EndpointSurface::KickChannelApi.env_var(),
                format!("{}/api/v2", server.uri()),
            ),
            (EndpointSurface::KickChatSocket.env_var(), socket_base),
        ];
        let endpoints = PlatformEndpoints::resolve(|variable| {
            overrides
                .iter()
                .find(|(name, _)| *name == variable)
                .map(|(_, base)| std::ffi::OsString::from(base))
        })
        .unwrap();
        let manager = Arc::new(KickCredentialsManager::new(
            &endpoints,
            InMemRepo::with_valid_creds(),
            "test_cid".to_owned(),
            "test_secret".to_owned(),
        ));
        let platform = KickPlatform::new(&endpoints, manager, Arc::new(GrantLimiter), ledger);
        (platform, server)
    }

    async fn next_banned_event(stream: &mut EventStream) -> Event {
        tokio::time::timeout(BANNED_STEP_DEADLINE, async {
            loop {
                let event = stream.recv().await.unwrap();
                if event.kind == USER_BANNED_EVENT_KIND {
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
                .any(|kind| kind == USER_BANNED_EVENT_KIND),
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
                    platform: forge_storage::ViewerPlatform::Kick,
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
}
