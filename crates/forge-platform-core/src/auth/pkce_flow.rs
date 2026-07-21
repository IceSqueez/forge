use std::time::Duration;

use serde::Deserialize;

use crate::error::PlatformError;

use super::local_callback::LocalCallbackDriver;

pub const REFRESH_BUFFER_SECS: u64 = 5 * 60;

#[derive(Debug, Clone, Copy)]
pub enum ReauthPolicy {
    AnyClientError,
    InvalidGrantOn400,
}

impl ReauthPolicy {
    fn requires_reauth(self, status: u16, body: &str) -> bool {
        match self {
            ReauthPolicy::AnyClientError => status == 400 || status == 401,
            ReauthPolicy::InvalidGrantOn400 => {
                status == 400
                    && serde_json::from_str::<serde_json::Value>(body)
                        .ok()
                        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned))
                        .as_deref()
                        == Some("invalid_grant")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PkceClientConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub authorize_endpoint: String,
    pub token_endpoint: String,
    pub scopes: Vec<String>,
    pub authorize_pre_redirect_params: Vec<(String, String)>,
    pub authorize_trailing_params: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct PkceRefreshConfig {
    pub platform: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub token_endpoint: String,
    pub reauth_policy: ReauthPolicy,
}

#[derive(Debug, Clone)]
pub struct PkceAuthorizeUrl {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub struct PkceTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
}

pub struct PkceFlow {
    config: PkceClientConfig,
    http: reqwest::Client,
    pending: Option<LocalCallbackDriver>,
}

impl PkceFlow {
    pub fn new(config: PkceClientConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            pending: None,
        }
    }

    pub fn redirect_path(&self) -> &'static str {
        super::local_callback::CALLBACK_PATH
    }

    /// Binds a loopback listener, generates PKCE + state, stores the driver, and
    /// returns the URL the caller should open in the user's browser. `extra_trailing_params`
    /// lets a caller add call-time-only query params (e.g. Google's optional `prompt=consent`)
    /// after the fixed PKCE fields.
    pub async fn start(
        &mut self,
        extra_trailing_params: &[(&str, &str)],
    ) -> Result<PkceAuthorizeUrl, PlatformError> {
        let driver = LocalCallbackDriver::bind().await?;
        let auth_url = build_authorize_url(&self.config, &driver, extra_trailing_params)?;
        self.pending = Some(driver);
        Ok(PkceAuthorizeUrl { auth_url })
    }

    /// Consumes the pending driver, awaits the loopback callback, and exchanges the
    /// authorization code for a token (PKCE `code_verifier`, no `client_secret` unless
    /// the config supplies one).
    pub async fn exchange(
        &mut self,
        timeout: Duration,
    ) -> Result<PkceTokenResponse, PlatformError> {
        let driver = self.pending.take().ok_or_else(|| PlatformError::Auth {
            reason: "exchange called before start".into(),
        })?;
        let redirect_uri = driver.redirect_uri().to_owned();
        let code_verifier = driver.code_verifier().to_owned();
        let callback = driver.await_callback(timeout).await?;

        let mut form: Vec<(&str, &str)> = vec![
            ("client_id", &self.config.client_id),
            ("code", &callback.code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", &code_verifier),
        ];
        if let Some(secret) = &self.config.client_secret {
            form.push(("client_secret", secret));
        }
        post_token_request(&self.http, &self.config.token_endpoint, &form).await
    }
}

pub struct PkceRefresher {
    http: reqwest::Client,
    config: PkceRefreshConfig,
}

impl PkceRefresher {
    pub fn new(config: PkceRefreshConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    /// Public-client `grant_type=refresh_token` POST (adds `client_secret` only when the
    /// config carries one). Maps to `PlatformError::ReauthRequired` per the configured
    /// `ReauthPolicy`, otherwise `PlatformError::Http`.
    pub async fn refresh(&self, refresh_token: &str) -> Result<PkceTokenResponse, PlatformError> {
        let mut form: Vec<(&str, &str)> = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.config.client_id),
        ];
        if let Some(secret) = &self.config.client_secret {
            form.push(("client_secret", secret));
        }

        let response = self
            .http
            .post(&self.config.token_endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|e| PlatformError::Network {
                reason: e.without_url().to_string(),
            })?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            if self.config.reauth_policy.requires_reauth(status, &body) {
                return Err(PlatformError::ReauthRequired {
                    platform: self.config.platform.clone(),
                });
            }
            return Err(PlatformError::Http { status, body });
        }

        let body = response.text().await.map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;
        parse_token_response(&body)
    }
}

fn build_authorize_url(
    config: &PkceClientConfig,
    driver: &LocalCallbackDriver,
    extra_trailing_params: &[(&str, &str)],
) -> Result<String, PlatformError> {
    let scope_string = config.scopes.join(" ");
    let mut params: Vec<(&str, &str)> =
        vec![("response_type", "code"), ("client_id", &config.client_id)];
    for (k, v) in &config.authorize_pre_redirect_params {
        params.push((k.as_str(), v.as_str()));
    }
    params.push(("redirect_uri", driver.redirect_uri()));
    params.push(("scope", scope_string.as_str()));
    params.push(("state", driver.state()));
    params.push(("code_challenge", driver.code_challenge()));
    params.push(("code_challenge_method", "S256"));
    for (k, v) in &config.authorize_trailing_params {
        params.push((k.as_str(), v.as_str()));
    }
    params.extend_from_slice(extra_trailing_params);

    let url =
        reqwest::Url::parse_with_params(&config.authorize_endpoint, &params).map_err(|e| {
            PlatformError::Auth {
                reason: format!("invalid authorize endpoint URL: {e}"),
            }
        })?;
    Ok(url.into())
}

async fn post_token_request(
    http: &reqwest::Client,
    token_endpoint: &str,
    form: &[(&str, &str)],
) -> Result<PkceTokenResponse, PlatformError> {
    let resp = http
        .post(token_endpoint)
        .form(form)
        .send()
        .await
        .map_err(|e| PlatformError::Network {
            reason: e.without_url().to_string(),
        })?;

    let status = resp.status().as_u16();
    if status != 200 {
        let body = resp.text().await.unwrap_or_default();
        return Err(PlatformError::Http { status, body });
    }
    let body = resp.text().await.map_err(|e| PlatformError::Network {
        reason: e.without_url().to_string(),
    })?;
    parse_token_response(&body)
}

fn parse_token_response(body: &str) -> Result<PkceTokenResponse, PlatformError> {
    let wire: WireTokenResponse = serde_json::from_str(body)?;
    Ok(PkceTokenResponse {
        access_token: wire.access_token,
        refresh_token: wire.refresh_token,
        expires_in: wire.expires_in,
    })
}

#[derive(Deserialize)]
struct WireTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}
