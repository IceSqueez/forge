use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::EndpointSurface;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::chat::{self, Viewer};
use super::config::FakeTwitchConfig;
use super::ids;
use super::ledger::Ledger;
use super::rest;
use super::socket;
use super::state::{Outbox, Shared};
use crate::EmulatorError;

const CHAT_MESSAGE_SUBSCRIPTION: &str = "channel.chat.message";
const SOCKET_PATH: &str = "/ws";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// Aborts its server tasks on drop; `shutdown` closes open sockets first.
pub struct FakeTwitch {
    shared: Arc<Shared>,
    api_base_url: String,
    shutdown: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl FakeTwitch {
    pub async fn start(config: FakeTwitchConfig) -> Result<Self, EmulatorError> {
        if config.keepalive_interval.is_zero() {
            return Err(EmulatorError::InvalidFakeConfig {
                reason: "keepalive interval must be non-zero".to_owned(),
            });
        }
        let api_listener = bind_loopback().await?;
        let socket_listener = bind_loopback().await?;
        let api_base_url = format!("http://{}", local_addr(&api_listener)?);
        let socket_url = format!("ws://{}{SOCKET_PATH}", local_addr(&socket_listener)?);

        let shared = Arc::new(Shared::new(config, socket_url));
        let (shutdown, shutdown_rx) = watch::channel(false);
        let mut rest_shutdown = shutdown_rx.clone();
        let router = rest::router(Arc::clone(&shared));
        let rest_task = tokio::spawn(async move {
            let _ = axum::serve(api_listener, router)
                .with_graceful_shutdown(async move {
                    let _ = rest_shutdown.changed().await;
                })
                .await;
        });
        let socket_task = tokio::spawn(socket::serve(
            socket_listener,
            Arc::clone(&shared),
            shutdown_rx,
        ));

        Ok(Self {
            shared,
            api_base_url,
            shutdown,
            tasks: vec![rest_task, socket_task],
        })
    }

    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }

    pub fn eventsub_ws_url(&self) -> &str {
        self.shared.socket_url()
    }

    /// Environment forge must be launched with so both Twitch surfaces reach this fake.
    pub fn endpoint_overrides(&self) -> [(&'static str, String); 2] {
        [
            (
                EndpointSurface::TwitchApi.env_var(),
                self.api_base_url.clone(),
            ),
            (
                EndpointSurface::TwitchEventSubSocket.env_var(),
                self.eventsub_ws_url().to_owned(),
            ),
        ]
    }

    pub fn ledger(&self) -> Ledger {
        self.shared.read(|inner| inner.ledger.clone())
    }

    /// Re-runs `probe` on every ledger change until it yields a value or `timeout` elapses.
    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&Ledger) -> Option<T>,
    ) -> Result<T, EmulatorError> {
        let mut changes = self.shared.changes();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(found) = self.shared.read(|inner| probe(&inner.ledger)) {
                return Ok(found);
            }
            match tokio::time::timeout_at(deadline, changes.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => {
                    return Err(EmulatorError::WaitTimeout {
                        what: what.to_owned(),
                    });
                }
            }
        }
    }

    /// Yields how many sessions the frame reached; conditions are not matched against `event`.
    pub async fn inject_notification(
        &self,
        subscription_type: &str,
        event: Value,
    ) -> Result<usize, EmulatorError> {
        let deliveries = self
            .shared
            .read(|inner| inner.notification_deliveries(subscription_type, &event));
        let reached = deliver(deliveries).await;
        if reached == 0 {
            return Err(EmulatorError::NotSubscribed {
                subscription_type: subscription_type.to_owned(),
            });
        }
        Ok(reached)
    }

    /// Forge surfaces the yielded id as the chat payload's platform message id.
    pub async fn inject_chat_message(
        &self,
        viewer: &Viewer,
        text: &str,
    ) -> Result<String, EmulatorError> {
        self.shared.mutate(|inner| inner.remember_viewer(viewer));
        let message_id = ids::uuid_like();
        let event = chat::chat_message_event(self.shared.config(), viewer, text, &message_id);
        self.inject_notification(CHAT_MESSAGE_SUBSCRIPTION, event)
            .await?;
        Ok(message_id)
    }

    /// Asks every live session to move to a fresh socket, which inherits its subscriptions.
    pub async fn inject_session_reconnect(&self) -> Result<usize, EmulatorError> {
        let socket_url = self.shared.socket_url();
        let deliveries = self
            .shared
            .read(|inner| inner.reconnect_deliveries(socket_url));
        match deliver(deliveries).await {
            0 => Err(EmulatorError::NoLiveSession),
            reached => Ok(reached),
        }
    }

    pub async fn shutdown(mut self) {
        let _ = self.shutdown.send(true);
        for mut task in std::mem::take(&mut self.tasks) {
            if tokio::time::timeout(SHUTDOWN_GRACE, &mut task)
                .await
                .is_err()
            {
                task.abort();
            }
        }
    }
}

impl Drop for FakeTwitch {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

async fn deliver(deliveries: Vec<(Outbox, String)>) -> usize {
    let mut reached = 0;
    for (outbox, frame) in deliveries {
        if outbox.send(frame).await.is_ok() {
            reached += 1;
        }
    }
    reached
}

async fn bind_loopback() -> Result<TcpListener, EmulatorError> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|e| EmulatorError::FakeBind {
            reason: e.to_string(),
        })
}

fn local_addr(listener: &TcpListener) -> Result<SocketAddr, EmulatorError> {
    listener.local_addr().map_err(|e| EmulatorError::FakeBind {
        reason: e.to_string(),
    })
}
