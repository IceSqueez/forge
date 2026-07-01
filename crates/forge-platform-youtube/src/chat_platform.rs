use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventStream};
use forge_platform_core::{
    AuthFlow, ChatPlatform, ConnectionState, PlatformCapabilities, PlatformError,
    connection_state_changed_event,
};
use futures::future::BoxFuture;
use tokio::sync::mpsc;
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
            cancel: Mutex::new(None),
        }
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

        publish_transition(&self.state, &self.events, ConnectionState::Connecting);

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
        let exit_events = Arc::clone(&self.events);
        let poller_cancel = cancel.clone();
        tokio::spawn(async move {
            if let Err(err) = poller.run(poller_cancel).await {
                tracing::warn!(error = %err, "youtube chat poller exited");
            }
            publish_transition(&exit_state, &exit_events, ConnectionState::Disconnected);
        });

        *self.cancel.lock().unwrap_or_else(|p| p.into_inner()) = Some(cancel);
        publish_transition(&self.state, &self.events, ConnectionState::Connected);
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), PlatformError> {
        let cancel = self.cancel.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
        publish_transition(&self.state, &self.events, ConnectionState::Disconnected);
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
        events.publish(connection_state_changed_event(PLATFORM_ID, new));
    }
}
