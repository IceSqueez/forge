use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialCheck {
    Accepted,
    MissingBearer,
    WrongBearer,
    MissingClientId,
    WrongClientId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    /// `None` for an empty body; a body that is not JSON is kept as a JSON string.
    pub body: Option<Value>,
    pub credentials: CredentialCheck,
    pub status: u16,
    pub response: Value,
    /// False when the fake has no model for this method and path, so its 404 is not Twitch's answer.
    pub modeled: bool,
}

/// One row per session holding the subscription: a reconnect copies rows under the same id.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedSubscription {
    pub id: String,
    pub session_id: String,
    pub subscription_type: String,
    pub version: String,
    pub condition: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
