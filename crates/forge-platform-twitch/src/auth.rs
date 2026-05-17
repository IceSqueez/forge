use std::time::Duration;

use forge_platform_core::{
    AuthFlow, PlatformError,
    oauth::{DeviceCodePoller, DeviceCodeRequest, DeviceCodeResponse},
};

pub const TWITCH_DEVICE_ENDPOINT: &str = "https://id.twitch.tv/oauth2/device";
pub const TWITCH_TOKEN_ENDPOINT: &str = "https://id.twitch.tv/oauth2/token";

pub const TWITCH_BROADCASTER_SCOPES: &[&str] = &[
    "chat:read",
    "chat:edit",
    "channel:read:subscriptions",
    "bits:read",
    "moderator:read:followers",
];

pub fn twitch_auth_flow() -> AuthFlow {
    AuthFlow::DeviceCode {
        user_code_endpoint: TWITCH_DEVICE_ENDPOINT.to_owned(),
        token_endpoint: TWITCH_TOKEN_ENDPOINT.to_owned(),
        scopes: TWITCH_BROADCASTER_SCOPES
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

pub async fn request_twitch_device_code(
    http: &reqwest::Client,
    client_id: &str,
) -> Result<DeviceCodeResponse, PlatformError> {
    DeviceCodePoller::request_device_code(
        http,
        TWITCH_DEVICE_ENDPOINT,
        DeviceCodeRequest {
            client_id: client_id.to_owned(),
            scopes: TWITCH_BROADCASTER_SCOPES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
    )
    .await
}

pub fn new_twitch_poller(
    client_id: String,
    device_code: String,
    interval: Duration,
    expires_in: Duration,
) -> DeviceCodePoller {
    DeviceCodePoller::new(
        client_id,
        TWITCH_TOKEN_ENDPOINT,
        device_code,
        interval,
        expires_in,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twitch_auth_flow_yields_device_code_variant() {
        let flow = twitch_auth_flow();
        let AuthFlow::DeviceCode {
            user_code_endpoint,
            token_endpoint,
            scopes,
        } = flow
        else {
            unreachable!("twitch_auth_flow must return DeviceCode variant");
        };
        assert_eq!(user_code_endpoint, TWITCH_DEVICE_ENDPOINT);
        assert_eq!(token_endpoint, TWITCH_TOKEN_ENDPOINT);
        assert_eq!(
            scopes,
            TWITCH_BROADCASTER_SCOPES
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn default_scopes_non_empty() {
        assert!(!TWITCH_BROADCASTER_SCOPES.is_empty());
    }
}
