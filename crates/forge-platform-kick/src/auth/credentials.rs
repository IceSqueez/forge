use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const CREDENTIAL_KEY: &str = "kick:broadcaster";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KickCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: u64,
    pub username: String,
    pub client_id: String,
    pub expires_at: OffsetDateTime,
}
