use std::time::Duration;

use forge_platform_core::{AuthFlow, PlatformError};
use forge_types::OAuthToken;

use twitch_api::HelixClient;
use twitch_api::twitch_oauth2::{
    AccessToken, ClientId, DeviceUserTokenBuilder, Scope, TwitchToken, UserToken,
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
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub id: String,
    pub login: String,
    pub display_name: String,
}

/// Public DCF code data returned to UI before polling.
#[derive(Debug, Clone)]
pub struct TwitchDeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: Duration,
    pub interval: Duration,
    pub device_code: String,
}

/// Bundle returned from the polling loop once the user completes authorization.
#[derive(Debug, Clone)]
pub struct TwitchAuthBundle {
    pub access_token: OAuthToken,
    pub user_info: UserInfo,
    pub client_id: String,
}

fn build_scopes() -> Vec<Scope> {
    TWITCH_BROADCASTER_SCOPES
        .iter()
        .map(|s| Scope::parse(*s))
        .collect()
}

/// Owns a `DeviceUserTokenBuilder` mid-flow and an HTTP client for both the
/// token endpoint and the subsequent Helix lookup.
pub struct TwitchAuthFlow {
    builder: DeviceUserTokenBuilder,
    client: reqwest::Client,
    helix: HelixClient<'static, reqwest::Client>,
    client_id: String,
}

impl TwitchAuthFlow {
    pub fn new(client_id: String) -> Self {
        let builder = DeviceUserTokenBuilder::new(ClientId::new(client_id.clone()), build_scopes());
        let client = reqwest::Client::new();
        let helix = HelixClient::with_client(client.clone());
        Self {
            builder,
            client,
            helix,
            client_id,
        }
    }

    /// Asks Twitch for a device code. Returns the user-facing code + URL.
    pub async fn start(&mut self) -> Result<TwitchDeviceCode, PlatformError> {
        let resp = self
            .builder
            .start(&self.client)
            .await
            .map_err(|e| PlatformError::Auth {
                reason: format!("device code request failed: {e}"),
            })?;
        Ok(TwitchDeviceCode {
            user_code: resp.user_code.clone(),
            verification_uri: resp.verification_uri.clone(),
            expires_in: Duration::from_secs(resp.expires_in),
            interval: Duration::from_secs(resp.interval),
            device_code: resp.device_code.clone(),
        })
    }

    /// Polls the token endpoint until the user authorizes, then resolves the
    /// signed-in user's Helix profile. Returns the full auth bundle.
    pub async fn wait_for_authorization(&mut self) -> Result<TwitchAuthBundle, PlatformError> {
        let user_token = self
            .builder
            .wait_for_code(&self.client, tokio::time::sleep)
            .await
            .map_err(|e| PlatformError::Auth {
                reason: format!("device code polling failed: {e}"),
            })?;

        let user_info = fetch_user_info_from_token(&user_token, &self.helix).await?;

        Ok(TwitchAuthBundle {
            access_token: OAuthToken::new(user_token.access_token.secret().to_owned()),
            user_info,
            client_id: self.client_id.clone(),
        })
    }
}

async fn fetch_user_info_from_token(
    token: &UserToken,
    helix: &HelixClient<'static, reqwest::Client>,
) -> Result<UserInfo, PlatformError> {
    let user = helix
        .get_user_from_id(&token.user_id, token)
        .await
        .map_err(|e| PlatformError::Auth {
            reason: format!("helix get_user failed: {e}"),
        })?
        .ok_or_else(|| PlatformError::Auth {
            reason: "helix returned empty user list".into(),
        })?;
    Ok(UserInfo {
        id: user.id.to_string(),
        login: user.login.to_string(),
        display_name: user.display_name.to_string(),
    })
}

/// Fetches the authorized user's Twitch ID + login via Helix GET /users using
/// a stored access token. Used by the chat-send bridge to resolve the
/// broadcaster ID on demand.
pub async fn fetch_user_info(
    token: &OAuthToken,
    client_id: &str,
) -> Result<UserInfo, PlatformError> {
    let access = AccessToken::new(token.expose().to_owned());
    let client_id_owned = ClientId::new(client_id.to_owned());
    let http = reqwest::Client::new();
    let helix = HelixClient::with_client(http);

    let user_token =
        UserToken::from_token(&helix, access)
            .await
            .map_err(|e| PlatformError::Auth {
                reason: format!("validate token failed: {e}"),
            })?;

    if user_token.client_id() != &client_id_owned {
        return Err(PlatformError::Auth {
            reason: "stored token client_id does not match configured client_id".into(),
        });
    }

    fetch_user_info_from_token(&user_token, &helix).await
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
        .map(str::to_owned)
        .or_else(|| compile_env.filter(|s| !s.is_empty()).map(str::to_owned))
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
                .map(|s| (*s).to_owned())
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
    fn build_scopes_returns_seven_entries() {
        assert_eq!(build_scopes().len(), TWITCH_BROADCASTER_SCOPES.len());
    }
}
