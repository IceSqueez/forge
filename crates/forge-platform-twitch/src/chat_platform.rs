use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_events::{EventPublisher, EventStream};
use forge_platform_core::{
    AuthFlow, ChatPlatform, ConnectionState, NON_HTTP_STATUS, PlatformCapabilities, PlatformError,
    RateLimiter,
};
use forge_storage::CredentialsRepo;
use tokio::sync::OnceCell;

use crate::auth::twitch_auth_flow;
use crate::builtin::ChatSessionConfig;
use crate::chat::{
    ChatSendError, TwitchChat, TwitchChatHandle, WhisperError, send_chat, send_chat_reply,
    send_whisper,
};
use crate::credentials::load;
use crate::credentials_manager::TwitchCredentialsManager;
use crate::event_channel::PlatformEventChannel;
use crate::helix::{HelixHttpTransport, HelixTokenRefresher, HelixTokenSource, HelixTransport};
use crate::lifecycle::TwitchLifecycle;
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
    lifecycle: TwitchLifecycle,
    handle: Mutex<Option<TwitchChatHandle>>,
    transport: OnceCell<Arc<dyn HelixTransport>>,
}

impl TwitchPlatform {
    pub fn new(
        config: ChatSessionConfig,
        creds: Arc<dyn CredentialsRepo>,
        credentials_manager: Arc<TwitchCredentialsManager>,
        tracker: SubscriptionTracker,
        rate_limiter: Arc<dyn RateLimiter>,
        lifecycle: TwitchLifecycle,
    ) -> Self {
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
            lifecycle,
            handle: Mutex::new(None),
            transport: OnceCell::new(),
        }
    }

    async fn helix_transport(&self) -> Result<Arc<dyn HelixTransport>, PlatformError> {
        self.transport
            .get_or_try_init(|| async {
                let publisher: Arc<dyn EventPublisher> = self.events.clone();
                let transport: Arc<dyn HelixTransport> = Arc::new(
                    HelixHttpTransport::new(
                        &self.config.endpoints,
                        Arc::clone(&self.rate_limiter),
                        publisher,
                        self.config.client_id.clone(),
                        Arc::clone(&self.credentials_manager) as Arc<dyn HelixTokenSource>,
                    )
                    .with_refresher(
                        Arc::clone(&self.credentials_manager) as Arc<dyn HelixTokenRefresher>
                    ),
                );
                Ok::<Arc<dyn HelixTransport>, PlatformError>(transport)
            })
            .await
            .cloned()
    }

    async fn sender(&self) -> Result<(String, Arc<dyn HelixTransport>), PlatformError> {
        if !self.capabilities.can_send_chat {
            return Err(PlatformError::Unsupported {
                feature: "chat.send".to_owned(),
            });
        }
        let cred = load(self.creds.as_ref())
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.to_string(),
            })?
            .ok_or_else(|| PlatformError::ReauthRequired {
                platform: PLATFORM_ID.to_owned(),
            })?;
        let transport = self.helix_transport().await?;
        Ok((cred.user_id, transport))
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
            previous.shutdown().await;
        }

        let publisher: Arc<dyn EventPublisher> = self.events.clone();
        let handle = TwitchChat::new(
            Arc::clone(&self.credentials_manager),
            self.config.clone(),
            publisher,
            self.tracker.clone(),
            self.lifecycle.clone(),
        )
        .start();
        *self.handle.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), PlatformError> {
        let handle = self.handle.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(handle) = handle {
            handle.shutdown().await;
        }
        Ok(())
    }

    async fn send_message(&self, _channel: &str, text: &str) -> Result<(), PlatformError> {
        let (user_id, transport) = self.sender().await?;
        send_chat(transport.as_ref(), &user_id, &user_id, text)
            .await
            .map(|_| ())
            .map_err(map_send_error)
    }

    async fn send_reply(
        &self,
        _channel: &str,
        reply_parent_message_id: &str,
        text: &str,
    ) -> Result<(), PlatformError> {
        let (user_id, transport) = self.sender().await?;
        send_chat_reply(
            transport.as_ref(),
            &user_id,
            &user_id,
            text,
            reply_parent_message_id,
        )
        .await
        .map(|_| ())
        .map_err(map_send_error)
    }

    async fn send_whisper(&self, recipient_login: &str, text: &str) -> Result<(), PlatformError> {
        let (user_id, transport) = self.sender().await?;
        send_whisper(transport.as_ref(), &user_id, recipient_login, text)
            .await
            .map_err(map_whisper_error)
    }

    fn events(&self) -> EventStream {
        self.events.subscribe()
    }
}

fn map_whisper_error(err: WhisperError) -> PlatformError {
    match err {
        WhisperError::Helix(helix) => map_send_error(ChatSendError::from(helix)),
        rejected => PlatformError::Http {
            status: NON_HTTP_STATUS,
            body: rejected.to_string(),
        },
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
        ChatSendError::MessageTooLong => PlatformError::Http {
            status: 413,
            body: err.to_string(),
        },
        ChatSendError::Http(body) => PlatformError::Http {
            status: NON_HTTP_STATUS,
            body,
        },
        ChatSendError::Dropped { code, message } => PlatformError::Http {
            status: NON_HTTP_STATUS,
            body: format!("{code}: {message}"),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::builtin::{HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW};
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, unreachable_twitch_endpoints,
        users_fixture,
    };
    use forge_platform_core::TokenBucketRateLimiter;

    fn platform_over(transport: &Arc<MockTransport>) -> TwitchPlatform {
        let creds: Arc<dyn CredentialsRepo> = Arc::new(MockCreds::with_identity());
        let platform = TwitchPlatform::new(
            ChatSessionConfig {
                client_id: "test-client".to_owned(),
                broadcaster_id: String::new(),
                user_id: String::new(),
                endpoints: unreachable_twitch_endpoints(),
            },
            Arc::clone(&creds),
            Arc::new(TwitchCredentialsManager::new(
                creds,
                "test-client".to_owned(),
            )),
            SubscriptionTracker::default(),
            Arc::new(TokenBucketRateLimiter::new(
                HELIX_BUDGET_CAPACITY,
                HELIX_BUDGET_WINDOW,
            )),
            TwitchLifecycle::new(),
        );
        platform
            .transport
            .set(Arc::clone(transport) as Arc<dyn HelixTransport>)
            .ok()
            .unwrap();
        platform
    }

    #[tokio::test]
    async fn send_whisper_posts_only_a_helix_whisper_from_the_signed_in_user() {
        let transport = Arc::new(MockTransport::returning_sequence(vec![
            users_fixture("555"),
            Ok(serde_json::Value::Null),
        ]));
        let platform = platform_over(&transport);

        platform.send_whisper("viewer", "psst").await.unwrap();

        let whisper = transport.last_request();
        assert_eq!(whisper.path, "/helix/whispers");
        assert!(
            whisper
                .query
                .contains(&("from_user_id".to_owned(), SELF_USER_ID.to_owned())),
            "got {:?}",
            whisper.query
        );
        for index in 0..transport.call_count() {
            assert_ne!(
                transport.request(index).path,
                "/helix/chat/messages",
                "a whisper must never reach public chat"
            );
        }
    }

    #[tokio::test]
    async fn rejected_whisper_maps_to_a_non_http_platform_error_without_a_helix_call() {
        let transport = Arc::new(MockTransport::returning(users_fixture("555")));
        let platform = platform_over(&transport);

        let err = platform.send_whisper("", "psst").await.unwrap_err();

        assert!(
            matches!(&err, PlatformError::Http { status: NON_HTTP_STATUS, body } if body.contains("recipient")),
            "got {err:?}"
        );
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn helix_whisper_failure_surfaces_status_without_token_or_url() {
        let transport = Arc::new(MockTransport::returning_sequence(vec![
            users_fixture("555"),
            Err(HelixError::Http {
                status: 403,
                body: "missing user:manage:whispers".to_owned(),
            }),
        ]));
        let platform = platform_over(&transport);

        let text = platform
            .send_whisper("viewer", "psst")
            .await
            .unwrap_err()
            .to_string();

        assert!(text.contains("403"), "status must surface: {text}");
        assert!(!text.contains(TOKEN_SENTINEL), "token leaked: {text}");
        assert!(!text.contains("api.twitch.tv"), "URL leaked: {text}");
    }

    #[tokio::test]
    async fn send_reply_posts_the_parent_id_to_helix_chat() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::json!({
            "data": [{ "message_id": "sent-1" }]
        }))));
        let platform = platform_over(&transport);

        platform
            .send_reply("twitch", "parent-7", "hello")
            .await
            .unwrap();

        let request = transport.last_request();
        assert_eq!(request.path, "/helix/chat/messages");
        assert_eq!(request.body.unwrap()["reply_parent_message_id"], "parent-7");
    }

    #[test]
    fn dropped_send_surfaces_code_and_message_in_the_platform_error() {
        let err = map_send_error(ChatSendError::Dropped {
            code: "channel_settings".to_owned(),
            message: "Followers-only mode is on.".to_owned(),
        });

        assert!(
            matches!(
                &err,
                PlatformError::Http { status: NON_HTTP_STATUS, body }
                    if body == "channel_settings: Followers-only mode is on."
            ),
            "got {err:?}"
        );
    }
}
