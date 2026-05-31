use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const CREDENTIAL_KEY: &str = "trovo:broadcaster";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrovoCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub client_id: String,
    pub username: String,
    pub user_id: String,
    pub expires_at: OffsetDateTime,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn credential_key_matches_expected() {
        assert_eq!(CREDENTIAL_KEY, "trovo:broadcaster");
    }

    #[test]
    fn credentials_serde_roundtrip() {
        let cred = TrovoCredentials {
            access_token: "tok".to_owned(),
            refresh_token: "ref".to_owned(),
            client_id: "cid".to_owned(),
            username: "streamer".to_owned(),
            user_id: "uid_42".to_owned(),
            expires_at: OffsetDateTime::UNIX_EPOCH,
        };
        let json = serde_json::to_string(&cred).unwrap();
        let back: TrovoCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, cred.user_id);
        assert_eq!(back.username, cred.username);
    }
}
