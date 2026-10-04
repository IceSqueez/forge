use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KickSurface {
    PublicApi,
    ChannelApi,
    OAuth,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KickCredentialCheck {
    Accepted,
    MissingBearer,
    WrongBearer,
    NotRequired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KickRequest {
    pub surface: KickSurface,
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub body: Option<Value>,
    pub credentials: KickCredentialCheck,
    pub status: u16,
    pub response: Value,
    pub modeled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PusherSession {
    pub id: u64,
    pub app_key: String,
    pub subscriptions: Vec<String>,
    pub live: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct KickLedger {
    pub requests: Vec<KickRequest>,
    pub sessions: Vec<PusherSession>,
}

impl KickLedger {
    pub fn unexpected_requests(&self) -> impl Iterator<Item = &KickRequest> {
        self.requests.iter().filter(|request| !request.modeled)
    }

    pub fn live_sessions(&self) -> impl Iterator<Item = &PusherSession> {
        self.sessions.iter().filter(|session| session.live)
    }

    pub fn joined(&self, channel: &str) -> Option<u64> {
        self.live_sessions()
            .filter(|session| session.subscriptions.iter().any(|joined| joined == channel))
            .map(|session| session.id)
            .last()
    }
}
