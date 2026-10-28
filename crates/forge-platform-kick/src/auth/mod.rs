pub(crate) mod credentials;
pub(crate) mod credentials_manager;
pub(crate) mod oauth_client;

use std::sync::Arc;
use std::time::Duration;

use forge_net::{Gateway, GatewayRequest};
use forge_platform_core::auth::{CALLBACK_PATH, PkceAuthorizeConfig, PkceLoopback};
use forge_platform_core::{AuthFlow, PlatformError};
use serde::Deserialize;
use time::OffsetDateTime;

use crate::auth::credentials_manager::KickCredentialsManager;
use crate::auth::oauth_client::KickOAuthClient;
use crate::gateway::{authorized, gateway_error, malformed};

pub const KICK_AUTHORIZE_ENDPOINT: &str = "https://id.kick.com/oauth/authorize";
pub const KICK_TOKEN_ENDPOINT: &str = "https://id.kick.com/oauth/token";

pub const KICK_OAUTH_CALLBACK_PORT: u16 = 53127;

const KICK_USERS_PATH: &str = "/users";
const EMPTY_USERS: &str = "users endpoint returned empty data array";
const NO_REFRESH_TOKEN: &str = "Kick token exchange did not return a refresh_token";
const LIFETIME_OUT_OF_RANGE: &str = "token lifetime out of range";

const KICK_SCOPES: &[&str] = &[
    "user:read",
    "channel:read",
    "channel:write",
    "channel:rewards:read",
    "channel:rewards:write",
    "chat:write",
    "moderation:chat_message:manage",
    "moderation:ban",
];

pub fn kick_auth_flow() -> AuthFlow {
    AuthFlow::LocalCallback {
        authorize_url: KICK_AUTHORIZE_ENDPOINT.to_owned(),
        token_endpoint: KICK_TOKEN_ENDPOINT.to_owned(),
        redirect_path: CALLBACK_PATH.to_owned(),
        scopes: KICK_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
    }
}

#[derive(Debug, Clone)]
pub struct LoopbackCode {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub struct KickAuthBundle {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: u64,
    pub username: String,
    pub client_id: String,
    pub expires_at: OffsetDateTime,
}

#[derive(Clone)]
pub struct KickSignIn {
    manager: Arc<KickCredentialsManager>,
}

impl KickSignIn {
    pub fn new(manager: Arc<KickCredentialsManager>) -> Self {
        Self { manager }
    }

    pub fn flow(&self) -> KickAuthFlow {
        KickAuthFlow::new(
            self.manager.oauth_client().clone(),
            self.manager.api().clone(),
        )
    }

    pub async fn save(&self, bundle: KickAuthBundle) -> Result<(), PlatformError> {
        self.manager.save_from_bundle(bundle).await
    }
}

pub struct KickAuthFlow {
    oauth: KickOAuthClient,
    api: Gateway,
    loopback: PkceLoopback,
}

impl KickAuthFlow {
    pub fn new(oauth: KickOAuthClient, api: Gateway) -> Self {
        let loopback = PkceLoopback::new(PkceAuthorizeConfig {
            client_id: oauth.client_id().to_owned(),
            authorize_endpoint: oauth.authorize_endpoint().to_owned(),
            scopes: KICK_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
            authorize_pre_redirect_params: vec![("redirect".to_owned(), "127.0.0.1".to_owned())],
            authorize_trailing_params: Vec::new(),
            preferred_port: Some(KICK_OAUTH_CALLBACK_PORT),
        });
        Self {
            oauth,
            api,
            loopback,
        }
    }

    pub async fn start(&mut self) -> Result<LoopbackCode, PlatformError> {
        let url = self.loopback.start(&[]).await?;
        Ok(LoopbackCode {
            auth_url: url.auth_url,
        })
    }

    pub async fn wait_for_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<KickAuthBundle, PlatformError> {
        let authorization = self.loopback.await_authorization(timeout).await?;
        let grant = self.oauth.exchange_code(&authorization).await?;
        let refresh_token = grant.refresh_token.ok_or_else(|| PlatformError::Auth {
            reason: NO_REFRESH_TOKEN.to_owned(),
        })?;
        let expires_at = expiry_after(grant.expires_in)?;

        let user = fetch_user_info(&self.api, &grant.access_token).await?;

        Ok(KickAuthBundle {
            access_token: grant.access_token,
            refresh_token,
            user_id: user.user_id,
            username: user.name,
            client_id: self.oauth.client_id().to_owned(),
            expires_at,
        })
    }
}

pub(crate) fn expiry_after(expires_in: Option<u64>) -> Result<OffsetDateTime, PlatformError> {
    let lifetime = i64::try_from(expires_in.unwrap_or(0)).unwrap_or(i64::MAX);
    OffsetDateTime::now_utc()
        .checked_add(time::Duration::seconds(lifetime))
        .ok_or_else(|| malformed(LIFETIME_OUT_OF_RANGE))
}

#[derive(Debug, Deserialize)]
struct UserRecord {
    user_id: u64,
    name: String,
}

#[derive(Debug, Deserialize)]
struct UsersResponse {
    data: Vec<UserRecord>,
}

async fn fetch_user_info(api: &Gateway, access_token: &str) -> Result<UserRecord, PlatformError> {
    let request = authorized(
        GatewayRequest::get(KICK_USERS_PATH),
        access_token,
        KICK_USERS_PATH,
    );
    let response = api.send(request).await.map_err(gateway_error)?;
    let parsed: UsersResponse = response.json().map_err(malformed)?;
    parsed
        .data
        .into_iter()
        .next()
        .ok_or_else(|| PlatformError::Http {
            status: response.status(),
            body: EMPTY_USERS.to_owned(),
        })
}

pub fn client_credentials() -> Option<(String, String)> {
    let client_id = resolve_credential(
        std::env::var("FORGE_KICK_CLIENT_ID").ok().as_deref(),
        option_env!("FORGE_KICK_CLIENT_ID"),
    )?;
    let client_secret = resolve_credential(
        std::env::var("FORGE_KICK_CLIENT_SECRET").ok().as_deref(),
        option_env!("FORGE_KICK_CLIENT_SECRET"),
    )?;
    Some((client_id, client_secret))
}

fn resolve_credential(
    runtime_env: Option<&str>,
    compile_env: Option<&'static str>,
) -> Option<String> {
    runtime_env
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| compile_env.filter(|s| !s.is_empty()).map(str::to_owned))
}
