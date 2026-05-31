use std::time::SystemTime;

use forge_platform_core::{AuthFlow, PlatformError};
use forge_types::OAuthToken;
use thiserror::Error;

pub const TROVO_AUTHORIZE_ENDPOINT: &str = "https://open.trovo.live/page/login.html";
pub const TROVO_TOKEN_ENDPOINT: &str = "https://open-api.trovo.live/openplatform/exchangetoken";
pub const TROVO_REFRESH_ENDPOINT: &str = "https://open-api.trovo.live/openplatform/refreshtoken";
pub const TROVO_USER_INFO_ENDPOINT: &str = "https://open-api.trovo.live/openplatform/getuserinfo";

pub const TROVO_BROADCASTER_SCOPES: &[&str] = &[
    "user_details_self",
    "channel_details_self",
    "chat_connect",
    "send_to_my_channel",
];

const CALLBACK_REDIRECT_PATH: &str = "/oauth/callback";

pub fn trovo_auth_flow() -> AuthFlow {
    AuthFlow::LocalCallback {
        authorize_url: TROVO_AUTHORIZE_ENDPOINT.to_owned(),
        token_endpoint: TROVO_TOKEN_ENDPOINT.to_owned(),
        redirect_path: CALLBACK_REDIRECT_PATH.to_owned(),
        scopes: TROVO_BROADCASTER_SCOPES
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

pub fn client_credentials() -> Option<(String, String)> {
    let id = option_env!("FORGE_TROVO_CLIENT_ID")?;
    let secret = option_env!("FORGE_TROVO_CLIENT_SECRET")?;
    if id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((id.to_owned(), secret.to_owned()))
}

#[derive(Debug, Clone)]
pub struct LoopbackCode {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub struct TrovoAuthBundle {
    pub access_token: OAuthToken,
    pub refresh_token: OAuthToken,
    pub username: String,
    pub user_id: String,
    pub client_id: String,
    pub expires_at: SystemTime,
}

#[derive(Debug, Error)]
pub enum TrovoAuthError {
    #[error("HTTP error {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("wait_for_authorization called before start")]
    NotStarted,
    #[error("loopback callback failure: {0}")]
    Loopback(#[from] PlatformError),
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_platform_core::AuthFlow;

    #[test]
    fn trovo_auth_flow_returns_local_callback_variant() {
        let flow = trovo_auth_flow();
        let AuthFlow::LocalCallback {
            authorize_url,
            token_endpoint,
            redirect_path,
            scopes,
        } = flow
        else {
            unreachable!("trovo_auth_flow must return LocalCallback variant");
        };
        assert_eq!(authorize_url, TROVO_AUTHORIZE_ENDPOINT);
        assert_eq!(token_endpoint, TROVO_TOKEN_ENDPOINT);
        assert_eq!(redirect_path, CALLBACK_REDIRECT_PATH);
        assert_eq!(
            scopes,
            TROVO_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn scope_list_is_non_empty() {
        assert!(!TROVO_BROADCASTER_SCOPES.is_empty());
    }

    #[test]
    fn scope_list_contains_required_scopes() {
        assert!(TROVO_BROADCASTER_SCOPES.contains(&"chat_connect"));
        assert!(TROVO_BROADCASTER_SCOPES.contains(&"send_to_my_channel"));
        assert!(TROVO_BROADCASTER_SCOPES.contains(&"user_details_self"));
    }

    #[test]
    fn client_credentials_result_is_consistent() {
        let result = client_credentials();
        if let Some((id, secret)) = result {
            assert!(!id.is_empty());
            assert!(!secret.is_empty());
        }
    }
}
