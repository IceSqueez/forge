use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::EndpointSurface;
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use super::config::FakeKickConfig;
use super::ledger::KickLedger;
use super::pusher::{self, SOCKET_PATH_PREFIX};
use super::rest::{self, CHANNEL_API_PREFIX, OAUTH_PREFIX, PUBLIC_API_PREFIX};
use super::state::Shared;
use crate::EmulatorError;

pub const CHAT_MESSAGE_EVENT: &str = "App\\Events\\ChatMessageEvent";

const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickChatter {
    pub user_id: u64,
    pub username: String,
}

pub struct FakeKick {
    shared: Arc<Shared>,
    http_base: String,
    socket_base: String,
    shutdown: watch::Sender<bool>,
    tasks: Vec<JoinHandle<()>>,
}

impl FakeKick {
    pub async fn start(config: FakeKickConfig) -> Result<Self, EmulatorError> {
        if config.chatroom_id == 0 {
            return Err(EmulatorError::InvalidFakeConfig {
                reason: "the Kick chatroom id must be non-zero".to_owned(),
            });
        }
        let http_listener = bind_loopback().await?;
        let socket_listener = bind_loopback().await?;
        let http_base = format!("http://{}", local_addr(&http_listener)?);
        let socket_base = format!("ws://{}{SOCKET_PATH_PREFIX}", local_addr(&socket_listener)?);

        let shared = Arc::new(Shared::new(config));
        let (shutdown, shutdown_rx) = watch::channel(false);
        let mut rest_shutdown = shutdown_rx.clone();
        let router = rest::router(Arc::clone(&shared));
        let rest_task = tokio::spawn(async move {
            let _ = axum::serve(http_listener, router)
                .with_graceful_shutdown(async move {
                    let _ = rest_shutdown.changed().await;
                })
                .await;
        });
        let socket_task = tokio::spawn(pusher::serve(
            socket_listener,
            Arc::clone(&shared),
            shutdown_rx,
        ));
        Ok(Self {
            shared,
            http_base,
            socket_base,
            shutdown,
            tasks: vec![rest_task, socket_task],
        })
    }

    pub fn endpoint_overrides(&self) -> [(&'static str, String); 4] {
        [
            (
                EndpointSurface::KickPublicApi.env_var(),
                format!("{}{PUBLIC_API_PREFIX}", self.http_base),
            ),
            (
                EndpointSurface::KickChannelApi.env_var(),
                format!("{}{CHANNEL_API_PREFIX}", self.http_base),
            ),
            (
                EndpointSurface::KickOAuth.env_var(),
                format!("{}{OAUTH_PREFIX}", self.http_base),
            ),
            (
                EndpointSurface::KickChatSocket.env_var(),
                self.socket_base.clone(),
            ),
        ]
    }

    pub fn config(&self) -> &FakeKickConfig {
        self.shared.config()
    }

    pub fn chat_channel(&self) -> String {
        self.shared.config().chat_channel()
    }

    pub fn ledger(&self) -> KickLedger {
        self.shared.read(|inner| inner.ledger.clone())
    }

    pub fn access_token(&self) -> String {
        self.shared.read(|inner| inner.tokens.access.clone())
    }

    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&KickLedger) -> Option<T>,
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

    pub async fn push_event(&self, event: &str, data: &Value) -> Result<usize, EmulatorError> {
        let channel = self.chat_channel();
        let outboxes = self.shared.read(|inner| inner.deliveries(&channel));
        let frame = pusher::frame(event, Some(&channel), data);
        let mut reached = 0;
        for outbox in outboxes {
            if outbox.send(frame.clone()).await.is_ok() {
                reached += 1;
            }
        }
        if reached == 0 {
            return Err(EmulatorError::KickChannelNotJoined { channel });
        }
        Ok(reached)
    }

    pub async fn inject_chat_message(
        &self,
        chatter: &KickChatter,
        text: &str,
    ) -> Result<String, EmulatorError> {
        let id = message_id();
        let data = json!({
            "id": id,
            "chatroom_id": self.shared.config().chatroom_id,
            "content": text,
            "type": "message",
            "created_at": OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default(),
            "sender": {
                "id": chatter.user_id,
                "username": chatter.username,
                "slug": chatter.username.to_lowercase(),
                "identity": { "color": "#75FD46", "badges": [] }
            }
        });
        self.push_event(CHAT_MESSAGE_EVENT, &data).await?;
        Ok(id)
    }

    pub fn set_live(&self, live: bool) {
        self.shared.mutate(|inner| inner.channel.live = live);
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

impl Drop for FakeKick {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub(crate) fn message_id() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
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
