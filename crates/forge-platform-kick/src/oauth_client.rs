use std::sync::Arc;
use std::time::Duration;

use forge_net::{
    CallClass, DEFAULT_RETRY_AFTER_SECS, FormBody, Gateway, GatewayBuildError, GatewayFailure,
    GatewayProfile, GatewayRequest, GatewayResponse, RateLimiter, Secret, ThrottleHintFamily,
};
use forge_platform_core::auth::PkceAuthorization;
use forge_platform_core::{EndpointSurface, PlatformEndpoints, PlatformError, reauth_required};
use serde::Deserialize;

use crate::api_gateway::{gateway_error, malformed};

const PLATFORM: &str = "kick";
const AUTHORIZE_PATH: &str = "/authorize";
const TOKEN_PATH: &str = "/token";

const AUTHORIZATION_CODE_GRANT_TYPE: &str = "authorization_code";
const REFRESH_GRANT_TYPE: &str = "refresh_token";

const BAD_REQUEST: u16 = reqwest::StatusCode::BAD_REQUEST.as_u16();
const NO_RETRY_AFTER_WAIT: Duration = Duration::from_secs(DEFAULT_RETRY_AFTER_SECS as u64);

pub const KICK_OAUTH_BUDGET_CAPACITY: u32 = 10;
pub const KICK_OAUTH_BUDGET_WINDOW: Duration = Duration::from_secs(60);

pub const KICK_OAUTH_GATEWAY_PROFILE: GatewayProfile = GatewayProfile::platform_api(
    CallClass::OAuth,
    ThrottleHintFamily::RetryAfter,
    NO_RETRY_AFTER_WAIT,
);

pub fn kick_oauth_gateway(
    endpoints: &PlatformEndpoints,
    limiter: Arc<dyn RateLimiter>,
) -> Result<Gateway, GatewayBuildError> {
    Gateway::new(
        endpoints.base_url(EndpointSurface::KickOAuth),
        limiter,
        KICK_OAUTH_GATEWAY_PROFILE,
        None,
    )
}

#[derive(Clone)]
pub struct KickOAuthClient {
    gateway: Gateway,
    authorize_endpoint: String,
    client_id: String,
    client_secret: String,
}

#[derive(Deserialize)]
pub(crate) struct TokenGrant {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) expires_in: Option<u64>,
}

impl KickOAuthClient {
    pub fn new(
        endpoints: &PlatformEndpoints,
        gateway: Gateway,
        client_id: String,
        client_secret: String,
    ) -> Self {
        Self {
            gateway,
            authorize_endpoint: format!(
                "{}{AUTHORIZE_PATH}",
                endpoints.base_url(EndpointSurface::KickOAuth)
            ),
            client_id,
            client_secret,
        }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub(crate) fn authorize_endpoint(&self) -> &str {
        &self.authorize_endpoint
    }

    pub(crate) async fn exchange_code(
        &self,
        authorization: &PkceAuthorization,
    ) -> Result<TokenGrant, PlatformError> {
        let form = FormBody::new()
            .field("client_id", self.client_id.as_str())
            .secret_field("code", Secret::new(authorization.code.as_str()))
            .field("grant_type", AUTHORIZATION_CODE_GRANT_TYPE)
            .field("redirect_uri", authorization.redirect_uri.as_str())
            .secret_field(
                "code_verifier",
                Secret::new(authorization.code_verifier.as_str()),
            )
            .secret_field("client_secret", Secret::new(self.client_secret.as_str()));
        let request = GatewayRequest::post(TOKEN_PATH)
            .label(TOKEN_PATH)
            .body(form);
        let response = self.gateway.send(request).await.map_err(gateway_error)?;
        decode(&response)
    }

    pub(crate) async fn refresh(&self, refresh_token: &str) -> Result<TokenGrant, PlatformError> {
        let form = FormBody::new()
            .field("grant_type", REFRESH_GRANT_TYPE)
            .secret_field("refresh_token", Secret::new(refresh_token))
            .field("client_id", self.client_id.as_str())
            .secret_field("client_secret", Secret::new(self.client_secret.as_str()));
        let request = GatewayRequest::post(TOKEN_PATH)
            .label(TOKEN_PATH)
            .body(form);
        match self.gateway.send(request).await {
            Ok(response) => decode(&response),
            Err(failure) if grant_rejected(&failure) => Err(reauth_required(PLATFORM)),
            Err(failure) => Err(gateway_error(failure)),
        }
    }
}

fn grant_rejected(failure: &GatewayFailure) -> bool {
    matches!(
        failure,
        GatewayFailure::Unauthorized { .. }
            | GatewayFailure::ClientRejected {
                status: BAD_REQUEST,
                ..
            }
    )
}

fn decode(response: &GatewayResponse) -> Result<TokenGrant, PlatformError> {
    response.json().map_err(malformed)
}
