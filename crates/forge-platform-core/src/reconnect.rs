use std::future::Future;
use std::sync::RwLock;
use std::time::Duration;

use forge_net::Backoff;
use forge_types::{MessageRef, RecoverRwLock};
use tokio::sync::watch;

pub struct AutoReconnectSwitch(watch::Sender<bool>);

impl AutoReconnectSwitch {
    pub fn new(enabled: bool) -> Self {
        Self(watch::Sender::new(enabled))
    }

    pub fn set(&self, enabled: bool) {
        self.0.send_replace(enabled);
    }

    pub fn is_enabled(&self) -> bool {
        *self.0.borrow()
    }

    pub fn watch(&self) -> AutoReconnectWatch {
        AutoReconnectWatch(self.0.subscribe())
    }
}

#[derive(Clone)]
pub struct AutoReconnectWatch(watch::Receiver<bool>);

impl AutoReconnectWatch {
    pub fn is_enabled(&mut self) -> bool {
        *self.0.borrow_and_update()
    }

    async fn reaches(&mut self, wanted: bool) -> bool {
        self.0.wait_for(|enabled| *enabled == wanted).await.is_ok()
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
                    if !park_until_auto_reconnect(session, &mut auto_reconnect).await {
                        return;
                    }
                    backoff.reset();
                }
            }
        }

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

pub fn record_connection_failure(slot: &RwLock<Option<MessageRef>>, failure: Option<MessageRef>) {
    *slot.write_recover() = failure;
}
