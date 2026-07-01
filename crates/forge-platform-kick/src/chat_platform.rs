use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventStream};
use forge_platform_core::{
    AuthFlow, ChatPlatform, ConnectionState, PlatformCapabilities, PlatformError, RateLimiter,
    connection_state_changed_event,
};
use tokio::sync::mpsc;

use crate::auth::kick_auth_flow;
use crate::capabilities::kick_capabilities;
use crate::chat::{KickChat, KickChatHandle};
use crate::credentials_manager::KickCredentialsManager;
use crate::error::KickError;
use crate::event_channel::PlatformEventChannel;
use crate::send::KickSendChat;

const PLATFORM_ID: &str = "kick";
const CHAT_FORWARD_CAPACITY: usize = 256;

pub struct KickPlatform {
    auth_flow: AuthFlow,
    capabilities: PlatformCapabilities,
    slug: String,
    events: Arc<PlatformEventChannel>,
    credentials_manager: Arc<KickCredentialsManager>,
    http: reqwest::Client,
    sender: KickSendChat,
    // Synchronized so `connection_state()` (a `&self` snapshot) and the async lifecycle
    // verbs share one chat handle without `&mut self`. The lock is never held across an
    // `.await`.
    handle: Mutex<Option<KickChatHandle>>,
}

impl KickPlatform {
    pub fn new(
        slug: String,
        credentials_manager: Arc<KickCredentialsManager>,
        rate_limiter: Arc<dyn RateLimiter>,
    ) -> Self {
        Self {
            auth_flow: kick_auth_flow(),
            capabilities: kick_capabilities(),
            slug,
            events: Arc::new(PlatformEventChannel::new()),
            credentials_manager,
            http: reqwest::Client::new(),
            sender: KickSendChat::new(rate_limiter),
            handle: Mutex::new(None),
        }
    }
}

#[async_trait]
impl ChatPlatform for KickPlatform {
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
        self.handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(KickChatHandle::connection_state)
            .unwrap_or(ConnectionState::Disconnected)
    }

    async fn connect(&self) -> Result<(), PlatformError> {
        let previous = self.handle.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(previous) = previous {
            previous.shutdown();
        }

        let (chat_tx, mut chat_rx) = mpsc::channel::<Event>(CHAT_FORWARD_CAPACITY);
        let handle = KickChat::new(self.slug.clone(), self.http.clone())
            .connect(chat_tx)
            .await
            .map_err(map_connect_error)?;

        let forward_events = self.events.clone();
        tokio::spawn(async move {
            while let Some(event) = chat_rx.recv().await {
                forward_events.publish(event);
            }
        });

        let state_events = self.events.clone();
        let mut state_rx = handle.state_receiver();
        tokio::spawn(async move {
            loop {
                let state = *state_rx.borrow_and_update();
                state_events.publish(connection_state_changed_event(PLATFORM_ID, state));
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
        self.sender.send(text, &token, creds.user_id).await
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
