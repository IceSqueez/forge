mod dispatch;
pub(crate) mod payload;
mod send;
mod session;
mod subscriber;

pub use send::{ChatSendError, SentMessageId, send_chat};
pub use session::ChatConnectionState;

use crate::credentials_manager::TwitchCredentialsManager;
use crate::lifecycle::TwitchLifecycle;
use crate::subscriptions::SubscriptionTracker;
use forge_events::EventPublisher;
use std::sync::Arc;
use tokio::sync::{oneshot, watch};

pub struct TwitchChat {
    manager: Arc<TwitchCredentialsManager>,
    client_id: String,
    broadcaster_id: String,
    user_id: String,
    bus: Arc<dyn EventPublisher>,
    tracker: SubscriptionTracker,
    lifecycle: TwitchLifecycle,
}

pub struct TwitchChatHandle {
    state_rx: watch::Receiver<ChatConnectionState>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl TwitchChat {
    pub fn new(
        manager: Arc<TwitchCredentialsManager>,
        client_id: String,
        broadcaster_id: String,
        user_id: String,
        bus: Arc<dyn EventPublisher>,
        tracker: SubscriptionTracker,
        lifecycle: TwitchLifecycle,
    ) -> Self {
        Self {
            manager,
            client_id,
            broadcaster_id,
            user_id,
            bus,
            tracker,
            lifecycle,
        }
    }

    pub fn start(self) -> TwitchChatHandle {
        let (sess, state_rx, shutdown_tx) = session::ChatSession::new(
            self.manager,
            self.client_id,
            self.broadcaster_id,
            self.user_id,
            self.bus,
            self.tracker,
            self.lifecycle,
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
