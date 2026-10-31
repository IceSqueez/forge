mod dedup;
mod dispatch;
pub(crate) mod payload;
mod send;
mod session;
mod subscriber;
mod topic_catalog;
mod whisper;

pub(crate) use send::send_chat_reply;
pub use send::{ChatSendError, SentMessageId, send_chat};
pub(crate) use send::{MAX_MESSAGE_LEN, refusal_text};
pub use session::ChatConnectionState;
pub(crate) use whisper::{MAX_WHISPER_CHARS, WhisperError, send_whisper, send_whisper_to};

use crate::bot_account::TwitchBotAccount;
use crate::config::{ChatSessionConfig, ChatTiming};
use crate::credentials_manager::TwitchCredentialsManager;
use crate::lifecycle::TwitchLifecycle;
use crate::subscriptions::SubscriptionTracker;
use forge_events::EventPublisher;
use forge_net::Gateway;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;
use tracing::warn;

pub use crate::config::SHUTDOWN_GRACE;

pub struct TwitchChat {
    manager: Arc<TwitchCredentialsManager>,
    helix: Gateway,
    config: ChatSessionConfig,
    bus: Arc<dyn EventPublisher>,
    tracker: SubscriptionTracker,
    lifecycle: TwitchLifecycle,
    bot: Option<Arc<TwitchBotAccount>>,
    timing: ChatTiming,
}

pub struct TwitchChatHandle {
    state_rx: watch::Receiver<ChatConnectionState>,
    shutdown_grace: Duration,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl TwitchChat {
    pub fn new(
        manager: Arc<TwitchCredentialsManager>,
        helix: Gateway,
        config: ChatSessionConfig,
        bus: Arc<dyn EventPublisher>,
        tracker: SubscriptionTracker,
        lifecycle: TwitchLifecycle,
    ) -> Self {
        Self {
            manager,
            helix,
            config,
            bus,
            tracker,
            lifecycle,
            bot: None,
            timing: ChatTiming::default(),
        }
    }

    pub fn with_timing(mut self, timing: ChatTiming) -> Self {
        self.timing = timing;
        self
    }

    pub(crate) fn with_bot(mut self, bot: Option<Arc<TwitchBotAccount>>) -> Self {
        self.bot = bot;
        self
    }

    pub fn start(self) -> TwitchChatHandle {
        let (sess, state_rx, shutdown_tx) = session::ChatSession::new(
            self.manager,
            self.helix,
            self.config,
            self.bus,
            self.tracker,
            self.lifecycle,
        );
        TwitchChatHandle::spawn(
            sess.with_bot(self.bot).with_timing(self.timing),
            state_rx,
            shutdown_tx,
            self.timing.shutdown_grace,
        )
    }

    pub(crate) fn start_reporting_to(
        self,
        state_tx: watch::Sender<ChatConnectionState>,
    ) -> TwitchChatHandle {
        let state_rx = state_tx.subscribe();
        let (sess, shutdown_tx) = session::ChatSession::reporting_to(
            self.manager,
            self.helix,
            self.config,
            self.bus,
            self.tracker,
            self.lifecycle,
            state_tx,
        );
        TwitchChatHandle::spawn(
            sess.with_bot(self.bot).with_timing(self.timing),
            state_rx,
            shutdown_tx,
            self.timing.shutdown_grace,
        )
    }
}

impl TwitchChatHandle {
    fn spawn(
        sess: session::ChatSession,
        state_rx: watch::Receiver<ChatConnectionState>,
        shutdown_tx: oneshot::Sender<()>,
        shutdown_grace: Duration,
    ) -> Self {
        Self {
            state_rx,
            shutdown_grace,
            shutdown_tx: Some(shutdown_tx),
            task: Some(tokio::spawn(sess.run())),
        }
    }

    pub fn connection_state(&self) -> ChatConnectionState {
        *self.state_rx.borrow()
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the session may already have ended"
            )]
            let _ = tx.send(());
        }
        let Some(mut task) = self.task.take() else {
            return;
        };
        if tokio::time::timeout(self.shutdown_grace, &mut task)
            .await
            .is_err()
        {
            warn!(
                grace_secs = self.shutdown_grace.as_secs(),
                "twitch chat session did not stop within the grace period; aborting it"
            );
            task.abort();
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the task was just aborted, so its result is a cancellation"
            )]
            let _ = task.await;
        }
    }
}
