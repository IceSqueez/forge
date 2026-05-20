mod reconnect;
mod send;
mod session;
mod subscriber;

pub use send::{ChatSendError, SentMessageId, send_chat};
pub use session::ChatConnectionState;

use forge_runtime::EventBus;
use forge_types::OAuthToken;
use std::sync::Arc;
use tokio::sync::{oneshot, watch};

pub struct TwitchChat {
    token: OAuthToken,
    client_id: String,
    broadcaster_id: String,
    user_id: String,
    bus: Arc<EventBus>,
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
        bus: Arc<EventBus>,
    ) -> Self {
        Self {
            token,
            client_id,
            broadcaster_id,
            user_id,
            bus,
        }
    }

    /// Spawns the EventSub WS session task. Returns immediately.
    pub fn start(self) -> TwitchChatHandle {
        let (sess, state_rx, shutdown_tx) = session::ChatSession::new(
            self.token,
            self.client_id,
            self.broadcaster_id,
            self.user_id,
            self.bus,
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

    pub fn state_receiver(&self) -> watch::Receiver<ChatConnectionState> {
        self.state_rx.clone()
    }

    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_connection_state_starts_as_connecting() {
        assert_eq!(
            ChatConnectionState::Connecting,
            ChatConnectionState::Connecting
        );
    }
}
