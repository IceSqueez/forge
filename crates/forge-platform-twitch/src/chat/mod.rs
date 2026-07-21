mod dispatch;
pub(crate) mod payload;
mod send;
mod session;
mod subscriber;

pub use send::{ChatSendError, SentMessageId, send_chat};
pub use session::ChatConnectionState;

use crate::subscriptions::SubscriptionTracker;
use forge_events::EventPublisher;
use forge_types::OAuthToken;
use std::sync::Arc;
use tokio::sync::{oneshot, watch};

pub struct TwitchChat {
    token: OAuthToken,
    client_id: String,
    broadcaster_id: String,
    user_id: String,
    bus: Arc<dyn EventPublisher>,
    tracker: SubscriptionTracker,
}

pub struct TwitchChatHandle {
    state_rx: watch::Receiver<ChatConnectionState>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl TwitchChat {
    pub fn new(
        token: OAuthToken,
        client_id: String,
        broadcaster_id: String,
        user_id: String,
        bus: Arc<dyn EventPublisher>,
        tracker: SubscriptionTracker,
    ) -> Self {
        Self {
            token,
            client_id,
            broadcaster_id,
            user_id,
            bus,
            tracker,
        }
    }

    pub fn start(self) -> TwitchChatHandle {
        let (sess, state_rx, shutdown_tx) = session::ChatSession::new(
            self.token,
            self.client_id,
            self.broadcaster_id,
            self.user_id,
            self.bus,
            self.tracker,
        );
        tokio::spawn(sess.run());
        TwitchChatHandle {
            state_rx,
            shutdown_tx: Some(shutdown_tx),
        }
    }
}

impl TwitchChatHandle {
    pub fn connection_state(&self) -> ChatConnectionState {
        *self.state_rx.borrow()
    }

    pub(crate) fn state_receiver(&self) -> watch::Receiver<ChatConnectionState> {
        self.state_rx.clone()
    }

    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}
