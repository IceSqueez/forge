use forge_platform_twitch::vocabulary::topics;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::watch;

use super::bans;
use super::chat::{self, Viewer};
use super::config::FakeTwitchConfig;
use super::ids;
use super::ledger::{Ledger, TappedRequest};
use super::rest;
use super::rewards::FakeReward;
use super::socket;
use super::state::{Outbox, Shared};
use crate::EmulatorError;
use crate::fakes::support::{ServerTasks, bind_loopback, local_addr, serve_http};

const CHAT_MESSAGE_SUBSCRIPTION: &str = topics::CHANNEL_CHAT_MESSAGE;
const FOLLOW_SUBSCRIPTION: &str = topics::CHANNEL_FOLLOW;
const SOCKET_PATH: &str = "/ws";

pub struct FakeTwitch {
    shared: Arc<Shared>,
    api_base_url: String,
    server: ServerTasks,
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
        let rest_shutdown = shutdown_rx.clone();
        let router = rest::router(Arc::clone(&shared));
        let rest_task = serve_http(api_listener, router, rest_shutdown);
        let socket_task = tokio::spawn(socket::serve(
            socket_listener,
            Arc::clone(&shared),
            shutdown_rx,
        ));

        Ok(Self {
            shared,
            api_base_url,
            server: ServerTasks::new(shutdown, vec![rest_task, socket_task]),
        })
    }

    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }

    pub fn eventsub_ws_url(&self) -> &str {
        self.shared.socket_url()
    }

    pub fn endpoint_overrides(&self) -> [(&'static str, String); 2] {
        [
            (
                forge_platform_twitch::endpoint_surfaces::TWITCH_API.env_var(),
                self.api_base_url.clone(),
            ),
            (
                forge_platform_twitch::endpoint_surfaces::TWITCH_EVENTSUB_SOCKET.env_var(),
                self.eventsub_ws_url().to_owned(),
            ),
        ]
    }

    pub fn tap_requests(&self) -> tokio::sync::mpsc::UnboundedReceiver<TappedRequest> {
        self.shared.mutate(super::state::Inner::tap_requests)
    }

    pub fn ledger(&self) -> Ledger {
        self.shared.read(|inner| inner.ledger.clone())
    }

    pub async fn wait_for<T>(
        &self,
        what: &str,
        timeout: Duration,
        mut probe: impl FnMut(&Ledger) -> Option<T>,
    ) -> Result<T, EmulatorError> {
        self.shared
            .wait_for(what, timeout, |inner| probe(&inner.ledger))
            .await
    }

    pub async fn inject_notification(
        &self,
        subscription_type: &str,
        event: Value,
    ) -> Result<usize, EmulatorError> {
        if subscription_type == FOLLOW_SUBSCRIPTION {
            self.shared.mutate(|inner| inner.remember_follow(&event));
        }
        if subscription_type == bans::BAN || subscription_type == bans::UNBAN {
            self.shared
                .mutate(|inner| inner.bans.remember(subscription_type, &event));
        }
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

    pub fn seed_dashboard_reward(&self, title: &str) -> FakeReward {
        self.shared
            .mutate(|inner| inner.rewards.seed_dashboard_reward(title))
    }

    pub fn rewards(&self) -> Vec<FakeReward> {
        self.shared.read(|inner| inner.rewards.snapshot())
    }

    pub async fn shutdown(self) {
        self.server.shutdown().await;
    }
}

pub(super) async fn deliver(deliveries: Vec<(Outbox, String)>) -> usize {
    let mut reached = 0;
    for (outbox, frame) in deliveries {
        if outbox.send(frame).await.is_ok() {
            reached += 1;
        }
    }
    reached
}
