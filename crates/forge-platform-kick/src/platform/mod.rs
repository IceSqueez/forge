mod sessions;

use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventChannel, EventSubscription};
use forge_platform_core::{
    ChatPlatform, ConnectionState, PlatformCapabilities, PlatformEndpoints, PlatformError,
};
use forge_storage::BanLedgerRepo;
use tokio::sync::{mpsc, watch};

use crate::ban_ledger::{USER_BANNED_EVENT_KIND, observed_ban};

use crate::api::chat_messages::KickSendChat;
use crate::auth::credentials_manager::KickCredentialsManager;
use crate::capabilities::kick_capabilities;
use crate::platform::sessions::{ChatSessions, PLATFORM_ID, SessionPublisher};
use crate::pusher::chat::{ChatLinkStatus, KickChat};
use crate::pusher::chatroom_lookup::ChannelInfoFetcher;
use crate::pusher::error::KickError;

const CHAT_FORWARD_CAPACITY: usize = 256;
pub(crate) const EVENT_CHANNEL_CAPACITY: usize = 256;

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
        if event.kind == USER_BANNED_EVENT_KIND {
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
