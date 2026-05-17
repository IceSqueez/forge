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
    "user:read:chat",
    "user:write:chat",
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
    client_id: &str,
) -> Result<DeviceCodeResponse, PlatformError> {
    DeviceCodePoller::request_device_code(
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

/// Priority: runtime env `FORGE_TWITCH_CLIENT_ID` → compile-time `option_env!` → `None`.
pub fn client_id() -> Option<String> {
    let runtime = std::env::var("FORGE_TWITCH_CLIENT_ID").ok();
    resolve_client_id(runtime.as_deref(), option_env!("FORGE_TWITCH_CLIENT_ID"))
}

fn resolve_client_id(
    runtime_env: Option<&str>,
    compile_env: Option<&'static str>,
) -> Option<String> {
    runtime_env
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
        .or_else(|| compile_env.filter(|s| !s.is_empty()).map(|s| s.to_owned()))
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

    #[test]
    fn client_id_prefers_runtime_over_compile_time() {
        assert_eq!(
            resolve_client_id(Some("runtime_id"), Some("compile_id")),
            Some("runtime_id".to_owned()),
        );
    }

    #[test]
    fn client_id_falls_back_to_compile_time_when_runtime_absent() {
        assert_eq!(
            resolve_client_id(None, Some("compile_id")),
            Some("compile_id".to_owned()),
        );
    }

    #[test]
    fn client_id_returns_none_when_both_absent() {
        assert_eq!(resolve_client_id(None, None), None);
    }

    #[test]
    fn client_id_treats_empty_runtime_as_absent() {
        assert_eq!(
            resolve_client_id(Some(""), Some("compile_id")),
            Some("compile_id".to_owned()),
        );
    }

    #[test]
    fn client_id_treats_empty_compile_time_as_absent() {
        assert_eq!(resolve_client_id(None, Some("")), None);
    }
}
