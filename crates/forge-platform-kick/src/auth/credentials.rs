use forge_types::OAuthToken;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const CREDENTIAL_KEY: &str = "kick:broadcaster";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KickCredentials {
    #[serde(serialize_with = "OAuthToken::serialize_exposed")]
    pub access_token: OAuthToken,
    #[serde(serialize_with = "OAuthToken::serialize_exposed")]
    pub refresh_token: OAuthToken,
    pub user_id: u64,
    pub username: String,
    pub client_id: String,
    pub expires_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCESS: &str = "kick-access-sentinel";
    const REFRESH: &str = "kick-refresh-sentinel";
    const SAVED_BUNDLE: &str = r#"{"access_token":"kick-access-sentinel","refresh_token":"kick-refresh-sentinel","user_id":18446744073709551615,"username":"стрімер","client_id":"client","expires_at":[2026,264,14,13,20,123456789,0,0,0]}"#;

    fn saved() -> KickCredentials {
        serde_json::from_str(SAVED_BUNDLE).expect("a saved bundle loads")
    }

    #[test]
    fn a_saved_bundle_loads_its_tokens_and_saves_back_byte_identical() {
        let loaded = saved();

        assert_eq!(
            (
                loaded.access_token.expose(),
                loaded.refresh_token.expose(),
                loaded.user_id
            ),
            (ACCESS, REFRESH, u64::MAX)
        );
        assert_eq!(serde_json::to_string(&loaded).unwrap(), SAVED_BUNDLE);
    }

    #[test]
    fn debug_of_saved_credentials_never_carries_a_token() {
        let loaded = saved();

        let rendered = format!("{loaded:?} {loaded:#?}");

        for secret in [ACCESS, REFRESH] {
            assert!(!rendered.contains(secret), "leaked: {rendered}");
        }
    }
}
