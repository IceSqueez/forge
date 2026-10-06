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
mod tests {
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
}
