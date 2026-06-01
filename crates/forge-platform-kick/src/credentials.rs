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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn credential_key_matches_expected() {
        assert_eq!(CREDENTIAL_KEY, "kick:broadcaster");
    }

    #[test]
    fn credentials_serde_roundtrip() {
        let cred = KickCredentials {
            access_token: "tok".to_owned(),
            refresh_token: "ref".to_owned(),
            user_id: 42,
            username: "streamer".to_owned(),
            client_id: "cid".to_owned(),
            expires_at: OffsetDateTime::UNIX_EPOCH,
        };
        let json = serde_json::to_string(&cred).unwrap();
        let back: KickCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, cred.user_id);
        assert_eq!(back.username, cred.username);
        assert_eq!(back.access_token, cred.access_token);
    }

    #[test]
    fn user_id_survives_roundtrip_as_u64() {
        let cred = KickCredentials {
            access_token: "a".to_owned(),
            refresh_token: "r".to_owned(),
            user_id: u64::MAX,
            username: "u".to_owned(),
            client_id: "c".to_owned(),
            expires_at: OffsetDateTime::UNIX_EPOCH,
        };
        let json = serde_json::to_string(&cred).unwrap();
        let back: KickCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, u64::MAX);
    }
}
