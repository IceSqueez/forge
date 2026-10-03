use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authentication {
    Pending,
    NotRequired,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObsSession {
    pub id: u64,
    pub authentication: Authentication,
    pub identified: bool,
    pub event_subscriptions: Option<u64>,
    pub closed: bool,
    pub close_code: Option<u16>,
}

impl ObsSession {
    pub fn is_live(&self) -> bool {
        self.identified && !self.closed
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObsRequest {
    pub session: u64,
    pub request_type: String,
    pub request_data: Value,
    pub code: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObsPushedEvent {
    pub event_type: String,
    pub data: Value,
    pub delivered_to: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ObsLedger {
    pub sessions: Vec<ObsSession>,
    pub requests: Vec<ObsRequest>,
    pub events: Vec<ObsPushedEvent>,
}

impl ObsLedger {
    pub fn live_sessions(&self) -> impl Iterator<Item = &ObsSession> {
        self.sessions.iter().filter(|session| session.is_live())
    }

    pub(crate) fn session_mut(&mut self, id: u64) -> Option<&mut ObsSession> {
        self.sessions.iter_mut().find(|session| session.id == id)
    }
}
