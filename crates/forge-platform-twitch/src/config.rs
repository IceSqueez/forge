use std::time::Duration;

use crate::account::broadcaster::BroadcasterIdentity;
use forge_net::{DEFAULT_BACKOFF_BASE, DEFAULT_BACKOFF_CAP};
use forge_platform_core::PlatformEndpoints;

pub const HELIX_BUDGET_CAPACITY: u32 = 800;
pub const HELIX_BUDGET_WINDOW: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(crate) struct ClientSettings {
    pub(crate) client_id: String,
    pub(crate) endpoints: PlatformEndpoints,
}

impl ClientSettings {
    pub(crate) fn session_for(&self, broadcaster: BroadcasterIdentity) -> ChatSessionConfig {
        ChatSessionConfig {
            client_id: self.client_id.clone(),
            broadcaster,
            endpoints: self.endpoints.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ChatSessionConfig {
    pub client_id: String,
    pub broadcaster: BroadcasterIdentity,
    pub endpoints: PlatformEndpoints,
}

pub(crate) const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const KEEPALIVE_GRACE: Duration = Duration::from_secs(5);
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug)]
pub struct ChatTiming {
    pub keepalive_timeout: Duration,
    pub shutdown_grace: Duration,
    pub reconnect_backoff_base: Duration,
    pub reconnect_backoff_cap: Duration,
}

impl Default for ChatTiming {
    fn default() -> Self {
        Self {
            keepalive_timeout: KEEPALIVE_TIMEOUT,
            shutdown_grace: SHUTDOWN_GRACE,
            reconnect_backoff_base: DEFAULT_BACKOFF_BASE,
            reconnect_backoff_cap: DEFAULT_BACKOFF_CAP,
        }
    }
}
