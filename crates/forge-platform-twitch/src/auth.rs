use std::time::Duration;

use forge_platform_core::{
    AuthFlow, PlatformError,
    oauth::{DeviceCodePoller, DeviceCodeRequest, DeviceCodeResponse},
};
use forge_types::OAuthToken;
use serde::Deserialize;

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

const HELIX_USERS_URL: &str = "https://api.twitch.tv/helix/users";

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: String,
    pub login: String,
    pub display_name: String,
}

#[derive(Deserialize)]
struct HelixUsersResponse {
    data: Vec<HelixUser>,
}

#[derive(Deserialize)]
struct HelixUser {
    id: String,
    login: String,
    display_name: String,
}

/// Fetches the authorized user's Twitch ID + login via Helix GET /users.
///
/// Returns the user's own info; `id` serves as both `broadcaster_id`
/// and `user_id` in EventSub conditions.
pub async fn fetch_user_info(
    token: &OAuthToken,
    client_id: &str,
) -> Result<UserInfo, PlatformError> {
    let http = reqwest::Client::new();
    let resp = http
        .get(HELIX_USERS_URL)
        .bearer_auth(token.expose())
        .header("Client-Id", client_id)
        .send()
        .await
        .map_err(|e| PlatformError::Network {
            reason: e.to_string(),
        })?;

    let status = resp.status().as_u16();
    tracing::debug!(status = %resp.status(), "helix user fetch");

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(PlatformError::Http { status, body });
    }

    let body: HelixUsersResponse = resp.json().await.map_err(|e| PlatformError::Network {
        reason: e.to_string(),
    })?;

    body.data
        .into_iter()
        .next()
        .map(|u| UserInfo {
            id: u.id,
            login: u.login,
            display_name: u.display_name,
        })
        .ok_or_else(|| PlatformError::Auth {
            reason: "helix returned empty user list".into(),
        })
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
#[allow(clippy::unwrap_used)]
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

    #[test]
    fn helix_users_response_parses_canonical_shape() {
        let json = r#"{"data":[{"id":"123","login":"foo","display_name":"Foo"}]}"#;
        let parsed: HelixUsersResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.data.len(), 1);
        assert_eq!(parsed.data[0].id, "123");
        assert_eq!(parsed.data[0].login, "foo");
        assert_eq!(parsed.data[0].display_name, "Foo");
    }

    #[test]
    fn helix_users_response_parses_empty_data() {
        let json = r#"{"data":[]}"#;
        let parsed: HelixUsersResponse = serde_json::from_str(json).unwrap();
        assert!(parsed.data.is_empty());
    }

    #[test]
    fn helix_users_response_rejects_missing_data_field() {
        let json = r#"{"other":"value"}"#;
        assert!(serde_json::from_str::<HelixUsersResponse>(json).is_err());
    }
}
