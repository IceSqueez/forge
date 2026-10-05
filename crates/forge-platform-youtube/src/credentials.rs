use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const CREDENTIAL_KEY: &str = "youtube:broadcaster";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub client_id: String,
    pub channel_id: String,
    pub channel_title: String,
    #[serde(default)]
    pub channel_handle: Option<String>,
    pub expires_at: OffsetDateTime,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn credentials_serde_roundtrip() {
        let cred = YoutubeCredentials {
            access_token: "tok".to_owned(),
            refresh_token: "ref".to_owned(),
            client_id: "cid".to_owned(),
            channel_id: "chan".to_owned(),
            channel_title: "My Channel".to_owned(),
            channel_handle: Some("@my_channel".to_owned()),
            expires_at: OffsetDateTime::UNIX_EPOCH,
        };
        let json = serde_json::to_string(&cred).unwrap();
        let back: YoutubeCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channel_id, cred.channel_id);
    }
}
