use forge_platform_core::PlatformEndpoints;
use forge_types::OAuthToken;
use reqwest::StatusCode;
use serde::Deserialize;
use tracing::debug;

use crate::auth::{TWITCH_BOT_SCOPES, twitch_validate_endpoint};
use crate::helix::bounded_http_client;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenVerdict {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Deserialize)]
struct ValidateResponse {
    client_id: String,
    #[serde(default)]
    scopes: Vec<String>,
}

pub(super) struct TokenValidator {
    http: reqwest::Client,
    endpoint: String,
    client_id: String,
}

impl TokenValidator {
    pub(super) fn new(endpoints: &PlatformEndpoints, client_id: String) -> Self {
        Self {
            http: bounded_http_client(),
            endpoint: twitch_validate_endpoint(endpoints),
            client_id,
        }
    }

    pub(super) async fn check(&self, token: &OAuthToken) -> TokenVerdict {
        let response = match self
            .http
            .get(&self.endpoint)
            .header("Authorization", format!("OAuth {}", token.expose()))
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                debug!(error = %e.without_url(), "bot token validation unreachable");
                return TokenVerdict::Unknown;
            }
        };
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return TokenVerdict::Invalid;
        }
        if !status.is_success() {
            return TokenVerdict::Unknown;
        }
        match response.json::<ValidateResponse>().await {
            Ok(body) => verdict_for(&body, &self.client_id),
            Err(e) => {
                debug!(error = %e.without_url(), "bot token validation body unreadable");
                TokenVerdict::Unknown
            }
        }
    }
}

fn verdict_for(body: &ValidateResponse, client_id: &str) -> TokenVerdict {
    let scopes_complete = TWITCH_BOT_SCOPES
        .iter()
        .all(|required| body.scopes.iter().any(|granted| granted == required));
    if body.client_id == client_id && scopes_complete {
        TokenVerdict::Valid
    } else {
        TokenVerdict::Invalid
    }
}
