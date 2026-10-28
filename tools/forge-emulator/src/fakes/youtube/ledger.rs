use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YouTubeSurface {
    DataApi,
    UploadApi,
    OAuth,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YouTubeCredentialCheck {
    Accepted,
    MissingBearer,
    WrongBearer,
    NotRequired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YouTubeRequest {
    pub surface: YouTubeSurface,
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub body: Option<Value>,
    pub credentials: YouTubeCredentialCheck,
    pub status: u16,
    pub response: Value,
    pub modeled: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct YouTubeLedger {
    pub requests: Vec<YouTubeRequest>,
}

impl YouTubeLedger {
    pub fn unexpected_requests(&self) -> impl Iterator<Item = &YouTubeRequest> {
        self.requests.iter().filter(|request| !request.modeled)
    }
}
