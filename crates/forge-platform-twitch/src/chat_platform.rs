use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{EventPublisher, EventStream};
use forge_platform_core::{
    AuthFlow, ChatPlatform, ConnectionState, PlatformCapabilities, PlatformError, RateLimiter,
};
use forge_storage::CredentialsRepo;
use tokio::sync::OnceCell;

use crate::auth::twitch_auth_flow;
use crate::builtin::ChatSessionConfig;
use crate::chat::{ChatSendError, TwitchChat, TwitchChatHandle, send_chat};
use crate::credentials::{CredentialsTokenSource, load};
use crate::credentials_manager::TwitchCredentialsManager;
use crate::event_channel::PlatformEventChannel;
use crate::helix::{HelixHttpTransport, HelixTransport};
use crate::subscriptions::SubscriptionTracker;

const PLATFORM_ID: &str = "twitch";

pub struct TwitchPlatform {
    auth_flow: AuthFlow,
    capabilities: PlatformCapabilities,
    config: ChatSessionConfig,
    events: Arc<PlatformEventChannel>,
    creds: Arc<dyn CredentialsRepo>,
    credentials_manager: Arc<TwitchCredentialsManager>,
    tracker: SubscriptionTracker,
    rate_limiter: Arc<dyn RateLimiter>,
    // std::sync::Mutex, not tokio: never held across an `.await`.
    handle: Mutex<Option<TwitchChatHandle>>,
    transport: OnceCell<Arc<dyn HelixTransport>>,
}

impl TwitchPlatform {
    pub fn new(
        config: ChatSessionConfig,
        creds: Arc<dyn CredentialsRepo>,
        tracker: SubscriptionTracker,
        rate_limiter: Arc<dyn RateLimiter>,
    ) -> Self {
        let credentials_manager = Arc::new(TwitchCredentialsManager::new(
            Arc::clone(&creds),
            config.client_id.clone(),
        ));
        Self {
            auth_flow: twitch_auth_flow(),
            capabilities: PlatformCapabilities {
                can_send_chat: true,
                can_moderate: true,
                can_subscribe_events: true,
                can_polls: true,
                can_predictions: true,
                can_channel_points: true,
                limited: false,
                limited_reason: None,
            },
            config,
            events: Arc::new(PlatformEventChannel::new()),
            creds,
            credentials_manager,
            tracker,
            rate_limiter,
            handle: Mutex::new(None),
            transport: OnceCell::new(),
        }
    }

    async fn helix_transport(&self) -> Result<Arc<dyn HelixTransport>, PlatformError> {
        self.transport
            .get_or_try_init(|| async {
                let publisher: Arc<dyn EventPublisher> = self.events.clone();
                let transport: Arc<dyn HelixTransport> = Arc::new(HelixHttpTransport::new(
                    Arc::clone(&self.rate_limiter),
                    publisher,
                    self.config.client_id.clone(),
                    Arc::new(CredentialsTokenSource::new(Arc::clone(&self.creds))),
                ));
                Ok::<Arc<dyn HelixTransport>, PlatformError>(transport)
            })
            .await
            .cloned()
    }
}

#[async_trait]
impl ChatPlatform for TwitchPlatform {
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
        let snapshot = self
            .handle
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(TwitchChatHandle::connection_state);
        match snapshot {
            Some(state) => state.to_connection_state(),
            None => ConnectionState::Disconnected,
        }
    }

    async fn connect(&self) -> Result<(), PlatformError> {
        load(self.creds.as_ref())
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?
            .ok_or_else(|| PlatformError::ReauthRequired {
                platform: PLATFORM_ID.to_owned(),
            })?;

        let previous = self.handle.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(previous) = previous {
            previous.shutdown();
        }

        let publisher: Arc<dyn EventPublisher> = self.events.clone();
        let handle = TwitchChat::new(
            Arc::clone(&self.credentials_manager),
            self.config.client_id.clone(),
            self.config.broadcaster_id.clone(),
            self.config.user_id.clone(),
            publisher,
            self.tracker.clone(),
        )
        .start();
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
        let transport = self.helix_transport().await?;
        send_chat(
            transport.as_ref(),
            &self.config.user_id,
            &self.config.user_id,
            text,
        )
        .await
        .map(|_| ())
        .map_err(map_send_error)
    }

    fn events(&self) -> EventStream {
        self.events.subscribe()
    }
}

fn map_send_error(err: ChatSendError) -> PlatformError {
    match err {
        ChatSendError::RateLimited => PlatformError::RateLimitExhausted,
        ChatSendError::ReauthRequired => PlatformError::ReauthRequired {
            platform: PLATFORM_ID.to_owned(),
        },
        ChatSendError::NotConnected => PlatformError::Network {
            reason: "not connected".to_owned(),
        },
        // 413 Payload Too Large: the 500-character cap is a client-side reject.
        ChatSendError::MessageTooLong => PlatformError::Http {
            status: 413,
            body: err.to_string(),
        },
        // The inner string is already URL/token-stripped by HelixError.
        ChatSendError::Http(body) => PlatformError::Http { status: 0, body },
    }
}
