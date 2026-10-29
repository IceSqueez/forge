use serde::{Deserialize, Serialize};

use crate::TypedEvent;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestFailed {
    pub endpoint: String,
    pub status_code: u16,
    pub body_snippet: String,
    pub retry_after_secs: Option<u64>,
}

impl TypedEvent for RequestFailed {
    const KIND: &'static str = "request.fail";
}
