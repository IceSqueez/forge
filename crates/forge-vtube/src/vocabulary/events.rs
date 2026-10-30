use forge_events::TypedEvent;
use serde::{Deserialize, Serialize};

use super::event_kinds;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VTubeDisconnectReason {
    AwaitingApproval,
    ConnectFailed,
    AuthRequired,
    AuthDenied,
    AuthTimeout,
    AuthFailed,
    SubscribeFailed,
    SocketClosed,
    Unresponsive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VTubeConnectionChanged {
    pub is_connected: bool,
    pub endpoint: String,
    pub reason: Option<VTubeDisconnectReason>,
    pub detail: Option<String>,
}

impl TypedEvent for VTubeConnectionChanged {
    const KIND: &'static str = event_kinds::CONNECTION_CHANGED;
}
