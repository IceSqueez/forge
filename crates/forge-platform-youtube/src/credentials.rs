use forge_types::OAuthToken;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const CREDENTIAL_KEY: &str = "youtube:broadcaster";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeCredentials {
    #[serde(serialize_with = "OAuthToken::serialize_exposed")]
    pub access_token: OAuthToken,
    #[serde(serialize_with = "OAuthToken::serialize_exposed")]
    pub refresh_token: OAuthToken,
    pub client_id: String,
    pub channel_id: String,
    pub channel_title: String,
    #[serde(default)]
    pub channel_handle: Option<String>,
    pub expires_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YoutubeBroadcaster {
    pub channel_id: String,
    pub channel_title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCESS: &str = "ya29.access-sentinel";
    const REFRESH: &str = "1//refresh-sentinel";
    const SAVED_BUNDLE: &str = r#"{"access_token":"ya29.access-sentinel","refresh_token":"1//refresh-sentinel","client_id":"client.apps.example","channel_id":"UC123","channel_title":"Канал","channel_handle":"@handle","expires_at":[2026,264,14,13,20,123456789,0,0,0]}"#;

    fn saved() -> YoutubeCredentials {
        serde_json::from_str(SAVED_BUNDLE).expect("a saved bundle loads")
    }

    #[test]
    fn a_saved_bundle_loads_its_tokens_and_saves_back_byte_identical() {
        let loaded = saved();

        assert_eq!(
            (loaded.access_token.expose(), loaded.refresh_token.expose()),
            (ACCESS, REFRESH)
        );
        assert_eq!(serde_json::to_string(&loaded).unwrap(), SAVED_BUNDLE);
    }

    #[test]
    fn a_bundle_saved_before_the_handle_existed_loads_without_one() {
        let without_handle = SAVED_BUNDLE.replace(r#""channel_handle":"@handle","#, "");

        let loaded: YoutubeCredentials = serde_json::from_str(&without_handle).unwrap();

        assert_eq!(loaded.channel_handle, None);
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
