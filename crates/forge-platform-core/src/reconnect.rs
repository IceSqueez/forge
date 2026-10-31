use std::future::Future;
use std::sync::RwLock;
use std::time::Duration;

use forge_net::Backoff;
use forge_types::{MessageRef, RecoverRwLock};
use tokio::sync::watch;

use crate::chat::AtomicConnectionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedialPhase {
    Dialing,
    BackingOff,
    Parked,
}

pub struct AutoReconnectSwitch {
    enabled: watch::Sender<bool>,
    phase: watch::Sender<RedialPhase>,
}

impl AutoReconnectSwitch {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: watch::Sender::new(enabled),
            phase: watch::Sender::new(RedialPhase::Dialing),
        }
    }

    pub fn set(&self, enabled: bool) {
        self.enabled.send_replace(enabled);
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.borrow()
    }

    pub fn watch(&self) -> AutoReconnectWatch {
        AutoReconnectWatch {
            enabled: self.enabled.subscribe(),
            phase: self.phase.clone(),
        }
    }

    pub async fn reached_phase(&self, wanted: RedialPhase) {
        if self
            .phase
            .subscribe()
            .wait_for(|phase| *phase == wanted)
            .await
            .is_err()
        {
            tracing::debug!("the reconnect switch closed before reaching a redial phase");
        }
    }
}

#[derive(Clone)]
pub struct AutoReconnectWatch {
    enabled: watch::Receiver<bool>,
    phase: watch::Sender<RedialPhase>,
}

impl AutoReconnectWatch {
    pub fn is_enabled(&mut self) -> bool {
        *self.enabled.borrow_and_update()
    }

    async fn reaches(&mut self, wanted: bool) -> bool {
        self.enabled
            .wait_for(|enabled| *enabled == wanted)
            .await
            .is_ok()
    }

    fn enter(&self, phase: RedialPhase) {
        self.phase.send_replace(phase);
    }
}

pub enum SessionEnd {
    Stopped,
    AttemptFailed,
    SessionLost,
}

pub trait ReconnectingSession: Send + Sync {
    fn stop_requested(&self) -> impl Future<Output = ()> + Send;

    fn retry_scheduled(&self, delay: Duration);

    fn stopped_while_retrying(&self);

    fn auto_reconnect_turned_off(&self);

    fn attempt(&mut self, reconnecting: bool) -> impl Future<Output = SessionEnd> + Send;
}

enum RetryWait {
    Elapsed,
    Disabled,
    Shutdown,
}

async fn wait_before_retry<S: ReconnectingSession>(
    delay: Duration,
    session: &S,
    auto_reconnect: &mut AutoReconnectWatch,
) -> RetryWait {
    tokio::select! {
        () = tokio::time::sleep(delay) => RetryWait::Elapsed,
        () = session.stop_requested() => RetryWait::Shutdown,
        _ = auto_reconnect.reaches(false) => RetryWait::Disabled,
    }
}

async fn park_until_auto_reconnect<S: ReconnectingSession>(
    session: &S,
    auto_reconnect: &mut AutoReconnectWatch,
) -> bool {
    tokio::select! {
        () = session.stop_requested() => false,
        resumed = auto_reconnect.reaches(true) => resumed,
    }
}

pub async fn run_reconnect_loop<S: ReconnectingSession>(
    session: &mut S,
    mut backoff: Backoff,
    mut auto_reconnect: AutoReconnectWatch,
) {
    let mut reconnecting = false;

    loop {
        if reconnecting {
            let delay = backoff.next_delay();
            let wait = if auto_reconnect.is_enabled() {
                session.retry_scheduled(delay);
                auto_reconnect.enter(RedialPhase::BackingOff);
                wait_before_retry(delay, session, &mut auto_reconnect).await
            } else {
                RetryWait::Disabled
            };
            match wait {
                RetryWait::Elapsed => {}
                RetryWait::Shutdown => {
                    session.stopped_while_retrying();
                    return;
                }
                RetryWait::Disabled => {
                    session.auto_reconnect_turned_off();
                    auto_reconnect.enter(RedialPhase::Parked);
                    if !park_until_auto_reconnect(session, &mut auto_reconnect).await {
                        return;
                    }
                    backoff.reset();
                }
            }
        }

        auto_reconnect.enter(RedialPhase::Dialing);
        match session.attempt(reconnecting).await {
            SessionEnd::Stopped => return,
            SessionEnd::AttemptFailed => reconnecting = true,
            SessionEnd::SessionLost => {
                backoff.reset();
                reconnecting = true;
            }
        }
    }
}

pub fn record_connection_failure(
    state: &AtomicConnectionState,
    slot: &RwLock<Option<MessageRef>>,
    failure: Option<MessageRef>,
) {
    let previous = std::mem::replace(&mut *slot.write_recover(), failure.clone());
    if previous != failure {
        state.mark_status_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ConnectionState;
    use forge_types::MessageCode;

    const LOST: MessageCode = MessageCode::from_static("test", "connection-lost");
    const REFUSED: MessageCode = MessageCode::from_static("test", "connection-refused");

    #[test]
    fn a_new_failure_moves_the_status_revision_and_the_same_failure_again_does_not() {
        let state = AtomicConnectionState::new(ConnectionState::Reconnecting);
        let slot = RwLock::new(None);
        let mut revisions = state.status_revisions();

        record_connection_failure(&state, &slot, None);
        assert!(!revisions.moved(), "no failure replaced by none");

        record_connection_failure(&state, &slot, Some(MessageRef::new(LOST)));
        assert!(revisions.moved(), "first failure");

        record_connection_failure(&state, &slot, Some(MessageRef::new(LOST)));
        assert!(!revisions.moved(), "retry failed the same way");

        record_connection_failure(&state, &slot, Some(MessageRef::new(REFUSED)));
        assert!(revisions.moved(), "failed a different way");

        record_connection_failure(&state, &slot, None);
        assert!(revisions.moved(), "failure cleared");
    }
}
