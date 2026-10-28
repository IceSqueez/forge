use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialCheck {
    Accepted,
    MissingBearer,
    WrongBearer,
    MissingClientId,
    WrongClientId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub body: Option<Value>,
    pub credentials: CredentialCheck,
    pub status: u16,
    pub response: Value,
    pub modeled: bool,
}

#[derive(Debug, Clone)]
pub struct TappedRequest {
    pub arrived: tokio::time::Instant,
    pub request: RecordedRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedSubscription {
    pub id: String,
    pub session_id: String,
    pub subscription_type: String,
    pub version: String,
    pub condition: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedSession {
    pub id: String,
    pub reconnected_from: Option<String>,
    pub connected_at: String,
    pub live: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ledger {
    pub requests: Vec<RecordedRequest>,
    pub subscriptions: Vec<RecordedSubscription>,
    pub sessions: Vec<RecordedSession>,
}

impl Ledger {
    pub fn unexpected_requests(&self) -> impl Iterator<Item = &RecordedRequest> {
        self.requests.iter().filter(|request| !request.modeled)
    }

    pub fn live_sessions(&self) -> impl Iterator<Item = &RecordedSession> {
        self.sessions.iter().filter(|session| session.live)
    }
}
