use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventStream};
use forge_platform_core::{
    AuthFlow, ChatPlatform, ConnectionState, PlatformCapabilities, PlatformError,
    connection_state_changed_event,
};
use futures::future::BoxFuture;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::active_broadcast_id::ActiveBroadcastIdHandle;
use crate::auth::youtube_auth_flow;
use crate::chat_poller::YoutubeChatPoller;
use crate::credentials_manager::YoutubeCredentialsManager;
use crate::event_channel::PlatformEventChannel;
use crate::live_chat_id::LiveChatIdHandle;
use crate::quota_state::QuotaState;
use crate::send_chat::YoutubeSendChat;

const PLATFORM_ID: &str = "youtube";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct YoutubePlatform {
    auth_flow: AuthFlow,
    capabilities: PlatformCapabilities,
    channel_id: String,
    events: Arc<PlatformEventChannel>,
    credentials_manager: Arc<YoutubeCredentialsManager>,
    sender: YoutubeSendChat,
    live_chat_id: LiveChatIdHandle,
    active_broadcast_id: ActiveBroadcastIdHandle,
    quota: Arc<tokio::sync::Mutex<QuotaState>>,
    // YouTube polls rather than holding a socket, so `connection_state()` reports this
    // coarse owned flag instead of a live transport state. Shared with the poller-exit
    // task; the lock is never held across an `.await`.
    state: Arc<Mutex<ConnectionState>>,
    // Outlives any single poller run, so a receiver taken once at construction
    // (e.g. by `YoutubeIntegrationBundle`) keeps observing state across every reconnect.
    state_tx: watch::Sender<ConnectionState>,
    // Lock never held across an `.await`.
    cancel: Mutex<Option<CancellationToken>>,
}

impl YoutubePlatform {
    pub fn new(
        channel_id: String,
        credentials_manager: Arc<YoutubeCredentialsManager>,
        live_chat_id: LiveChatIdHandle,
        active_broadcast_id: ActiveBroadcastIdHandle,
        quota: Arc<tokio::sync::Mutex<QuotaState>>,
    ) -> Self {
        let sender = YoutubeSendChat::new(
            token_source(Arc::clone(&credentials_manager)),
            live_chat_id.clone(),
            Arc::clone(&quota),
        );
        let (state_tx, _) = watch::channel(ConnectionState::Disconnected);
        Self {
            auth_flow: youtube_auth_flow(),
            capabilities: youtube_capabilities(),
            channel_id,
            events: Arc::new(PlatformEventChannel::new()),
            credentials_manager,
            sender,
            live_chat_id,
            active_broadcast_id,
            quota,
            state: Arc::new(Mutex::new(ConnectionState::Disconnected)),
            state_tx,
            cancel: Mutex::new(None),
        }
    }

    pub fn active_broadcast_id(&self) -> ActiveBroadcastIdHandle {
        self.active_broadcast_id.clone()
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ConnectionState> {
        self.state_tx.subscribe()
    }
}

#[async_trait]
impl ChatPlatform for YoutubePlatform {
    fn platform_id(&self) -> &'static str {
        PLATFORM_ID
    }

    fn auth_flow(&self) -> &AuthFlow {
        &self.auth_flow
    }

    fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    fn connection_state(&self) -> ConnectionState {
        *self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    async fn connect(&self) -> Result<(), PlatformError> {
        let previous = self.cancel.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(previous) = previous {
            previous.cancel();
        }

        publish_transition(
            &self.state,
            &self.state_tx,
            &self.events,
            ConnectionState::Connecting,
        );

        let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
        let forward_events = Arc::clone(&self.events);
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                forward_events.publish(event);
            }
        });

        let cancel = CancellationToken::new();
        let poller = YoutubeChatPoller::new(
            token_source(Arc::clone(&self.credentials_manager)),
            tx,
            self.channel_id.clone(),
            self.live_chat_id.clone(),
            self.active_broadcast_id.clone(),
            Arc::clone(&self.quota),
        );

        let exit_state = Arc::clone(&self.state);
        let exit_state_tx = self.state_tx.clone();
        let exit_events = Arc::clone(&self.events);
        let poller_cancel = cancel.clone();
        tokio::spawn(async move {
            if let Err(err) = poller.run(poller_cancel).await {
                tracing::warn!(error = %err, "youtube chat poller exited");
            }
            publish_transition(
                &exit_state,
                &exit_state_tx,
                &exit_events,
                ConnectionState::Disconnected,
            );
        });

        *self.cancel.lock().unwrap_or_else(|p| p.into_inner()) = Some(cancel);
        publish_transition(
            &self.state,
            &self.state_tx,
            &self.events,
            ConnectionState::Connected,
        );
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), PlatformError> {
        let cancel = self.cancel.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
        publish_transition(
            &self.state,
            &self.state_tx,
            &self.events,
            ConnectionState::Disconnected,
        );
        Ok(())
    }

    async fn send_message(&self, _channel: &str, text: &str) -> Result<(), PlatformError> {
        if !self.capabilities.can_send_chat {
            return Err(PlatformError::Unsupported {
                feature: "chat.send".to_owned(),
            });
        }
        self.credentials_manager
            .load()
            .await?
            .ok_or_else(|| PlatformError::ReauthRequired {
                platform: PLATFORM_ID.to_owned(),
            })?;
        self.sender.send(text).await
    }

    fn events(&self) -> EventStream {
        self.events.subscribe()
    }
}

fn youtube_capabilities() -> PlatformCapabilities {
    PlatformCapabilities {
        can_send_chat: true,
        can_moderate: true,
        can_subscribe_events: false,
        can_polls: false,
        can_predictions: false,
        can_channel_points: false,
        limited: false,
        limited_reason: None,
    }
}

fn token_source(manager: Arc<YoutubeCredentialsManager>) -> TokenSource {
    Arc::new(move || {
        let manager = Arc::clone(&manager);
        Box::pin(async move { manager.get_valid_access_token().await })
    })
}

fn publish_transition(
    state: &Mutex<ConnectionState>,
    state_tx: &watch::Sender<ConnectionState>,
    events: &PlatformEventChannel,
    new: ConnectionState,
) {
    let changed = {
        let mut guard = state.lock().unwrap_or_else(|p| p.into_inner());
        if *guard == new {
            false
        } else {
            *guard = new;
            true
        }
    };
    if changed {
        let _ = state_tx.send(new);
        events.publish(connection_state_changed_event(PLATFORM_ID, new));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use async_trait::async_trait;
    use forge_events::Event;
    use forge_platform_core::CONNECTION_STATE_CHANGED_KIND;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use time::OffsetDateTime;

    use super::*;
    use crate::auth::GoogleAuthFlow;

    struct EmptyRepo;
    #[async_trait]
    impl CredentialsRepo for EmptyRepo {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(None)
        }
        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
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

    // A platform with no stored credentials: the poller's token source fails, so it
    // never touches the network and only cancellation drives it to exit.
    fn platform() -> YoutubePlatform {
        let manager = Arc::new(YoutubeCredentialsManager::new(
            Arc::new(EmptyRepo),
            GoogleAuthFlow::new("test_cid".to_owned(), "test_secret".to_owned()),
        ));
        YoutubePlatform::new(
            "UCtest".to_owned(),
            manager,
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            Arc::new(tokio::sync::Mutex::new(QuotaState::default())),
        )
    }

    fn state_of(event: &Event) -> Option<String> {
        if event.kind != CONNECTION_STATE_CHANGED_KIND {
            return None;
        }
        Some(event.payload["state"].as_str().unwrap().to_owned())
    }

    #[test]
    fn connection_state_is_disconnected_before_connect() {
        assert_eq!(platform().connection_state(), ConnectionState::Disconnected);
    }

    #[tokio::test]
    async fn connect_publishes_connecting_then_connected_optimistically() {
        // The flag is coarse: `connect` reports Connected on spawn, not on a confirmed
        // broadcast. The observable contract is the ordered Connecting -> Connected pair.
        let p = platform();
        let mut stream = p.events();
        p.connect().await.unwrap();
        assert_eq!(
            state_of(&stream.recv().await.unwrap()).as_deref(),
            Some("connecting")
        );
        assert_eq!(
            state_of(&stream.recv().await.unwrap()).as_deref(),
            Some("connected")
        );
    }

    #[tokio::test]
    async fn send_message_without_credentials_requires_reauth() {
        let p = platform();
        let err = p.send_message("chan", "hello").await.unwrap_err();
        assert!(
            matches!(&err, PlatformError::ReauthRequired { platform } if platform == "youtube"),
            "expected ReauthRequired {{ platform: youtube }}, got {err:?}"
        );
    }

    #[tokio::test]
    async fn explicit_disconnect_deduplicates_the_redundant_poller_exit_transition() {
        // connect -> Connecting,Connected; disconnect -> Disconnected AND cancels the
        // poller. When the cancelled poller exits it also asks to publish Disconnected,
        // but `publish_transition` must suppress that redundant same-state event.
        // Without the dedup guard the drained sequence would carry a 4th Disconnected.
        let p = platform();
        let mut stream = p.events();
        p.connect().await.unwrap();
        p.disconnect().await.unwrap();
        drop(p); // release the platform's own channel handle so the drain can terminate

        let mut states = Vec::new();
        while let Ok(event) = stream.recv().await {
            if let Some(state) = state_of(&event) {
                states.push(state);
            }
        }
        assert_eq!(states, ["connecting", "connected", "disconnected"]);
    }
}
