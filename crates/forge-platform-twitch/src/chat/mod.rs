mod dedup;
mod dispatch;
pub(crate) mod payload;
mod send;
mod session;
mod subscriber;

pub use send::{ChatSendError, SentMessageId, send_chat};
pub use session::ChatConnectionState;

use crate::builtin::ChatSessionConfig;
use crate::credentials_manager::TwitchCredentialsManager;
use crate::lifecycle::TwitchLifecycle;
use crate::subscriptions::SubscriptionTracker;
use forge_events::EventPublisher;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;
use tracing::warn;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

pub struct TwitchChat {
    manager: Arc<TwitchCredentialsManager>,
    config: ChatSessionConfig,
    bus: Arc<dyn EventPublisher>,
    tracker: SubscriptionTracker,
    lifecycle: TwitchLifecycle,
}

pub struct TwitchChatHandle {
    state_rx: watch::Receiver<ChatConnectionState>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl TwitchChat {
    pub fn new(
        manager: Arc<TwitchCredentialsManager>,
        config: ChatSessionConfig,
        bus: Arc<dyn EventPublisher>,
        tracker: SubscriptionTracker,
        lifecycle: TwitchLifecycle,
    ) -> Self {
        Self {
            manager,
            config,
            bus,
            tracker,
            lifecycle,
        }
    }

    pub fn start(self) -> TwitchChatHandle {
        let (sess, state_rx, shutdown_tx) = session::ChatSession::new(
            self.manager,
            self.config,
            self.bus,
            self.tracker,
            self.lifecycle,
        );
        TwitchChatHandle::spawn(sess, state_rx, shutdown_tx)
    }

    pub(crate) fn start_reporting_to(
        self,
        state_tx: watch::Sender<ChatConnectionState>,
    ) -> TwitchChatHandle {
        let state_rx = state_tx.subscribe();
        let (sess, shutdown_tx) = session::ChatSession::reporting_to(
            self.manager,
            self.config,
            self.bus,
            self.tracker,
            self.lifecycle,
            state_tx,
        );
        TwitchChatHandle::spawn(sess, state_rx, shutdown_tx)
    }
}

impl TwitchChatHandle {
    fn spawn(
        sess: session::ChatSession,
        state_rx: watch::Receiver<ChatConnectionState>,
        shutdown_tx: oneshot::Sender<()>,
    ) -> Self {
        Self {
            state_rx,
            shutdown_tx: Some(shutdown_tx),
            task: Some(tokio::spawn(sess.run())),
        }
    }

    pub fn connection_state(&self) -> ChatConnectionState {
        *self.state_rx.borrow()
    }

    /// Resolves after the session task has ended; a task outliving the grace period is aborted.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let Some(mut task) = self.task.take() else {
            return;
        };
        if tokio::time::timeout(SHUTDOWN_GRACE, &mut task)
            .await
            .is_err()
        {
            warn!(
                grace_secs = SHUTDOWN_GRACE.as_secs(),
                "twitch chat session did not stop within the grace period; aborting it"
            );
            task.abort();
            let _ = task.await;
        }
    }
}
