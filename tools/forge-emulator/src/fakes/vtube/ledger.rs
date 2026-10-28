use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenCheck {
    Pending,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VTubeSession {
    pub id: u64,
    pub authentication: TokenCheck,
    pub token_requested: bool,
    pub subscriptions: Vec<String>,
    pub closed: bool,
}

impl VTubeSession {
    pub fn is_live(&self) -> bool {
        self.authentication == TokenCheck::Accepted && !self.closed
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VTubeRequest {
    pub session: u64,
    pub message_type: String,
    pub data: Value,
    pub error_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VTubePushedEvent {
    pub event_name: String,
    pub data: Value,
    pub delivered_to: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VTubeLedger {
    pub sessions: Vec<VTubeSession>,
    pub requests: Vec<VTubeRequest>,
    pub events: Vec<VTubePushedEvent>,
}

impl VTubeLedger {
    pub fn live_sessions(&self) -> impl Iterator<Item = &VTubeSession> {
        self.sessions.iter().filter(|session| session.is_live())
    }

    pub(crate) fn session_mut(&mut self, id: u64) -> Option<&mut VTubeSession> {
        self.sessions.iter_mut().find(|session| session.id == id)
    }
}
