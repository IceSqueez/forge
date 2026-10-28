use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{EventChannel, EventPublisher, EventSubscription};
use forge_net::Gateway;
use forge_platform_core::{
    ActingSender, ChatPlatform, ConnectionState, NON_HTTP_STATUS, PlatformCapabilities,
    PlatformError, SenderPreference,
};
use forge_storage::CredentialsRepo;
use tokio::sync::{Mutex, OnceCell, watch};

use crate::account::identity::SelfIdentity;
use crate::bot_account::TwitchBotAccount;
use crate::chat::{ChatConnectionState, ChatSendError, TwitchChat, TwitchChatHandle, WhisperError};
use crate::chat_sender::{ChatDelivery, ChatSenders, SendFailure};
use crate::config::ChatSessionConfig;
use crate::credentials::load;
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{HelixHttpTransport, HelixTokenRefresher, HelixTokenSource, HelixTransport};
use crate::lifecycle::TwitchLifecycle;
use crate::subscriptions::SubscriptionTracker;

const PLATFORM_ID: &str = "twitch";
pub(crate) const EVENT_CHANNEL_CAPACITY: usize = 4096;

pub struct TwitchPlatform {
    capabilities: PlatformCapabilities,
    config: ChatSessionConfig,
    events: Arc<EventChannel>,
    creds: Arc<dyn CredentialsRepo>,
    credentials_manager: Arc<TwitchCredentialsManager>,
    tracker: SubscriptionTracker,
    helix: Gateway,
    lifecycle: TwitchLifecycle,
    state_tx: watch::Sender<ChatConnectionState>,
    session: Mutex<Option<TwitchChatHandle>>,
    transport: OnceCell<Arc<dyn HelixTransport>>,
    bot: Option<Arc<TwitchBotAccount>>,
}

impl TwitchPlatform {
    pub fn new(
        config: ChatSessionConfig,
        creds: Arc<dyn CredentialsRepo>,
        credentials_manager: Arc<TwitchCredentialsManager>,
        tracker: SubscriptionTracker,
        helix: Gateway,
        lifecycle: TwitchLifecycle,
    ) -> Self {
        Self {
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
            events: Arc::new(EventChannel::new(EVENT_CHANNEL_CAPACITY)),
            creds,
            credentials_manager,
            tracker,
            helix,
            lifecycle,
            state_tx: watch::channel(ChatConnectionState::Disconnected).0,
            session: Mutex::new(None),
            transport: OnceCell::new(),
            bot: None,
        }
    }

    pub fn with_bot(mut self, bot: Arc<TwitchBotAccount>) -> Self {
        self.bot = Some(bot);
        self
    }

    pub fn bot_account(&self) -> Option<&Arc<TwitchBotAccount>> {
        self.bot.as_ref()
    }

    pub(crate) fn publisher(&self) -> Arc<dyn EventPublisher> {
        self.events.clone()
    }

    pub(crate) fn session_states(&self) -> watch::Receiver<ChatConnectionState> {
        self.state_tx.subscribe()
    }

    pub(crate) fn config(&self) -> &ChatSessionConfig {
        &self.config
    }

    pub(crate) fn credentials(&self) -> &Arc<dyn CredentialsRepo> {
        &self.creds
    }

    pub(crate) fn credentials_manager(&self) -> &Arc<TwitchCredentialsManager> {
        &self.credentials_manager
    }

    pub(crate) fn tracker(&self) -> &SubscriptionTracker {
        &self.tracker
    }

    pub(crate) fn helix_gateway(&self) -> &Gateway {
        &self.helix
    }

    pub(crate) fn lifecycle(&self) -> &TwitchLifecycle {
        &self.lifecycle
    }

    async fn helix_transport(&self) -> Result<Arc<dyn HelixTransport>, PlatformError> {
        self.transport
            .get_or_try_init(|| async {
                let transport: Arc<dyn HelixTransport> = Arc::new(
                    HelixHttpTransport::new(
                        self.helix.clone(),
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

    async fn senders(&self) -> Result<ChatSenders, PlatformError> {
        if !self.capabilities.can_send_chat {
            return Err(PlatformError::Unsupported {
                feature: "chat.send".to_owned(),
            });
        }
        Ok(ChatSenders::new(
            self.helix_transport().await?,
            Arc::new(SelfIdentity::new(Arc::clone(&self.creds))),
        )
        .with_bot(self.bot.clone()))
    }

    async fn deliver(
        &self,
        delivery: ChatDelivery<'_>,
        preference: SenderPreference,
    ) -> Result<ActingSender, PlatformError> {
        self.senders()
            .await?
            .deliver(delivery, preference, self.events.as_ref())
            .await
            .map_err(map_send_failure)
    }
}

#[async_trait]
impl ChatPlatform for TwitchPlatform {
    fn connection_state(&self) -> ConnectionState {
        self.state_tx.borrow().to_connection_state()
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

        let mut session = self.session.lock().await;
        if let Some(previous) = session.take() {
            previous.shutdown().await;
        }
        *session = Some(
            TwitchChat::new(
                Arc::clone(&self.credentials_manager),
                self.helix.clone(),
                self.config.clone(),
                self.publisher(),
                self.tracker.clone(),
                self.lifecycle.clone(),
            )
            .with_bot(self.bot.clone())
            .start_reporting_to(self.state_tx.clone()),
        );
        Ok(())
    }

    async fn disconnect(&self) -> Result<(), PlatformError> {
        let mut session = self.session.lock().await;
        if let Some(handle) = session.take() {
            handle.shutdown().await;
        }
        Ok(())
    }

    async fn send_message(&self, channel: &str, text: &str) -> Result<(), PlatformError> {
        self.send_message_as(channel, text, SenderPreference::Default)
            .await
            .map(|_| ())
    }

    async fn send_reply(
        &self,
        channel: &str,
        reply_parent_message_id: &str,
        text: &str,
    ) -> Result<(), PlatformError> {
        self.send_reply_as(
            channel,
            reply_parent_message_id,
            text,
            SenderPreference::Default,
        )
        .await
        .map(|_| ())
    }

    async fn send_whisper(&self, recipient_login: &str, text: &str) -> Result<(), PlatformError> {
        self.send_whisper_as(recipient_login, text, SenderPreference::Default)
            .await
            .map(|_| ())
    }

    async fn send_message_as(
        &self,
        _channel: &str,
        text: &str,
        sender: SenderPreference,
    ) -> Result<Option<ActingSender>, PlatformError> {
        self.deliver(ChatDelivery::Message { text }, sender)
            .await
            .map(Some)
    }

    async fn send_reply_as(
        &self,
        _channel: &str,
        reply_parent_message_id: &str,
        text: &str,
        sender: SenderPreference,
    ) -> Result<Option<ActingSender>, PlatformError> {
        self.deliver(
            ChatDelivery::Reply {
                text,
                parent_message_id: reply_parent_message_id,
            },
            sender,
        )
        .await
        .map(Some)
    }

    async fn send_whisper_as(
        &self,
        recipient_login: &str,
        text: &str,
        sender: SenderPreference,
    ) -> Result<Option<ActingSender>, PlatformError> {
        self.deliver(
            ChatDelivery::WhisperToLogin {
                login: recipient_login,
                text,
            },
            sender,
        )
        .await
        .map(Some)
    }

    fn events(&self) -> EventSubscription {
        self.events.subscribe()
    }
}

fn map_send_failure(failure: SendFailure) -> PlatformError {
    match failure {
        SendFailure::BotNotConnected(_) => PlatformError::Auth {
            reason: failure.to_string(),
        },
        SendFailure::BotPhoneUnverified => PlatformError::Http {
            status: NON_HTTP_STATUS,
            body: failure.to_string(),
        },
        SendFailure::BotNotModerator => PlatformError::Http {
            status: NON_HTTP_STATUS,
            body: failure.to_string(),
        },
        SendFailure::PermissionDenied(_) | SendFailure::Refused(_) => PlatformError::Http {
            status: NON_HTTP_STATUS,
            body: failure.to_string(),
        },
        SendFailure::Helix(err) => map_send_error(ChatSendError::from(err)),
        SendFailure::Chat(err) => map_send_error(err),
        SendFailure::Whisper(err) => map_whisper_error(err),
        SendFailure::Credentials(reason) => PlatformError::Network { reason },
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
        ChatSendError::Forbidden(reason) => PlatformError::Http {
            status: forge_net::status::FORBIDDEN,
            body: reason,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, unreachable_twitch_endpoints,
        users_fixture,
    };

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
                crate::sub_actions::test_support::unreachable_oauth_client("test-client"),
                creds,
            )),
            SubscriptionTracker::default(),
            crate::sub_actions::test_support::unreachable_helix_gateway(),
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
    async fn raid_sized_burst_published_before_the_first_read_arrives_whole_and_in_order() {
        const RAID_BURST: usize = 1000;
        let platform = platform_over(&Arc::new(MockTransport::returning(users_fixture("555"))));
        let mut events = platform.events();
        let publisher = platform.publisher();

        for index in 0..RAID_BURST {
            publisher.publish(forge_events::Event::new(
                forge_events::EventSource::Twitch,
                "twitch.channel.chat.message",
                serde_json::json!({ "index": index }),
            ));
        }

        for index in 0..RAID_BURST {
            let event = events.recv().await.unwrap();
            assert_eq!(event.payload["index"], index);
        }
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

    #[test]
    fn bot_send_failures_map_to_platform_errors_that_keep_their_cause() {
        use crate::bot_account::BotUnavailable;

        type Expectation = fn(&PlatformError) -> bool;
        let cases: [(SendFailure, Expectation); 4] = [
            (
                SendFailure::BotNotConnected(BotUnavailable::NotConnected),
                |e| matches!(e, PlatformError::Auth { reason } if reason.starts_with("bot not connected")),
            ),
            (
                SendFailure::BotPhoneUnverified,
                |e| matches!(e, PlatformError::Http { status: NON_HTTP_STATUS, body } if body.contains("verified phone")),
            ),
            (
                SendFailure::Chat(ChatSendError::Forbidden("banned".to_owned())),
                |e| matches!(e, PlatformError::Http { status: 403, body } if body == "banned"),
            ),
            (SendFailure::Chat(ChatSendError::RateLimited), |e| {
                matches!(e, PlatformError::RateLimitExhausted)
            }),
        ];

        for (failure, expected) in cases {
            let label = failure.to_string();
            let mapped = map_send_failure(failure);
            assert!(expected(&mapped), "{label} mapped to {mapped:?}");
        }
    }
}
