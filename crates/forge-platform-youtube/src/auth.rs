use std::time::{Duration, SystemTime};

use forge_platform_core::AuthFlow;
use forge_types::OAuthToken;
use thiserror::Error;

pub const GOOGLE_DEVICE_ENDPOINT: &str = "https://oauth2.googleapis.com/device/code";
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

pub const YOUTUBE_BROADCASTER_SCOPES: &[&str] =
    &["https://www.googleapis.com/auth/youtube.force-ssl"];

pub fn youtube_auth_flow() -> AuthFlow {
    AuthFlow::DeviceCode {
        user_code_endpoint: GOOGLE_DEVICE_ENDPOINT.to_owned(),
        token_endpoint: GOOGLE_TOKEN_ENDPOINT.to_owned(),
        scopes: YOUTUBE_BROADCASTER_SCOPES
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

#[derive(Debug, Clone)]
pub struct YoutubeDeviceCode {
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: Duration,
    pub interval: Duration,
    pub device_code: String,
}

#[derive(Debug, Clone)]
pub struct YoutubeAuthBundle {
    pub access_token: OAuthToken,
    pub refresh_token: OAuthToken,
    pub channel_id: String,
    pub channel_title: String,
    pub client_id: String,
    pub expires_at: SystemTime,
}

#[allow(dead_code)]
pub struct GoogleAuthFlow {
    client: reqwest::Client,
    client_id: String,
    client_secret: String,
}

impl GoogleAuthFlow {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id,
            client_secret,
        }
    }

    pub async fn start(&self) -> Result<YoutubeDeviceCode, YoutubeAuthError> {
        Err(YoutubeAuthError::NotImplemented)
    }

    pub async fn wait_for_authorization(
        &self,
        _device_code: &str,
        _interval: Duration,
    ) -> Result<YoutubeAuthBundle, YoutubeAuthError> {
        Err(YoutubeAuthError::NotImplemented)
    }
}

#[derive(Debug, Error)]
pub enum YoutubeAuthError {
    #[error("HTTP error {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("access denied by user")]
    AccessDenied,
    #[error("device code or token has expired")]
    ExpiredToken,
    #[error("not implemented")]
    NotImplemented,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use forge_platform_core::AuthFlow;

    #[test]
    fn youtube_auth_flow_returns_device_code_variant() {
        let flow = youtube_auth_flow();
        let AuthFlow::DeviceCode {
            user_code_endpoint,
            token_endpoint,
            scopes,
        } = flow
        else {
            unreachable!("youtube_auth_flow must return DeviceCode variant");
        };
        assert_eq!(user_code_endpoint, GOOGLE_DEVICE_ENDPOINT);
        assert_eq!(token_endpoint, GOOGLE_TOKEN_ENDPOINT);
        assert_eq!(
            scopes,
            YOUTUBE_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn scope_list_contains_force_ssl() {
        assert!(
            YOUTUBE_BROADCASTER_SCOPES
                .iter()
                .any(|s| s.contains("youtube.force-ssl"))
        );
    }

    #[tokio::test]
    async fn start_returns_not_implemented_at_p2() {
        let flow = GoogleAuthFlow::new("test".to_owned(), "test".to_owned());
        let result = flow.start().await;
        assert!(matches!(result, Err(YoutubeAuthError::NotImplemented)));
    }
}
