use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

pub const CREDENTIAL_KEY: &str = "youtube:broadcaster";

/// Storage key for quota telemetry in `SettingsRepo` (not `CredentialsRepo`).
pub const QUOTA_KEY: &str = "youtube:quota";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub client_id: String,
    pub channel_id: String,
    pub channel_title: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeQuotaState {
    pub used_today: u32,
    pub peak_seen: u32,
    pub reset_at: OffsetDateTime,
    pub last_reset_date: Date,
}

impl Default for YoutubeQuotaState {
    fn default() -> Self {
        Self {
            used_today: 0,
            peak_seen: 0,
            reset_at: OffsetDateTime::UNIX_EPOCH,
            last_reset_date: Date::MIN,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn credential_key_matches_expected() {
        assert_eq!(CREDENTIAL_KEY, "youtube:broadcaster");
    }

    #[test]
    fn quota_key_matches_expected() {
        assert_eq!(QUOTA_KEY, "youtube:quota");
    }

    #[test]
    fn quota_state_default_has_zero_usage() {
        let state = YoutubeQuotaState::default();
        assert_eq!(state.used_today, 0);
        assert_eq!(state.peak_seen, 0);
    }

    #[test]
    fn credentials_serde_roundtrip() {
        let cred = YoutubeCredentials {
            access_token: "tok".to_owned(),
            refresh_token: "ref".to_owned(),
            client_id: "cid".to_owned(),
            channel_id: "chan".to_owned(),
            channel_title: "My Channel".to_owned(),
            expires_at: OffsetDateTime::UNIX_EPOCH,
        };
        let json = serde_json::to_string(&cred).unwrap();
        let back: YoutubeCredentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channel_id, cred.channel_id);
    }
}
