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
    pub preferred_port: Option<u16>,
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

    /// `extra_trailing_params` lets a caller add call-time-only query params (e.g. Google's
    /// optional `prompt=consent`) after the fixed PKCE fields.
    pub async fn start(
        &mut self,
        extra_trailing_params: &[(&str, &str)],
    ) -> Result<PkceAuthorizeUrl, PlatformError> {
        let driver = LocalCallbackDriver::bind(self.config.preferred_port).await?;
        let auth_url = build_authorize_url(&self.config, &driver, extra_trailing_params)?;
        self.pending = Some(driver);
        Ok(PkceAuthorizeUrl { auth_url })
    }

    /// Uses PKCE `code_verifier`; no `client_secret` unless the config supplies one.
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

    /// Maps to `PlatformError::ReauthRequired` per the configured `ReauthPolicy`, otherwise
    /// `PlatformError::Http`.
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config(authorize_endpoint: String, token_endpoint: String) -> PkceClientConfig {
        PkceClientConfig {
            client_id: "test_client".to_owned(),
            client_secret: None,
            authorize_endpoint,
            token_endpoint,
            scopes: vec!["scope:a".to_owned(), "scope:b".to_owned()],
            authorize_pre_redirect_params: Vec::new(),
            authorize_trailing_params: Vec::new(),
            preferred_port: None,
        }
    }

    #[tokio::test]
    async fn authorize_url_contains_required_pkce_params_and_no_secret() {
        let mut flow = PkceFlow::new(config(
            "https://example.com/authorize".to_owned(),
            "https://example.com/token".to_owned(),
        ));
        let url = flow.start(&[]).await.unwrap().auth_url;
        assert!(url.starts_with("https://example.com/authorize"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=test_client"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code_challenge="));
        assert!(url.contains("state="));
        assert!(url.contains("scope=scope%3Aa+scope%3Ab"));
        assert!(!url.to_lowercase().contains("client_secret"));
    }

    #[tokio::test]
    async fn authorize_url_places_pre_redirect_params_before_redirect_uri() {
        let mut cfg = config(
            "https://example.com/authorize".to_owned(),
            "https://example.com/token".to_owned(),
        );
        cfg.authorize_pre_redirect_params = vec![("redirect".to_owned(), "127.0.0.1".to_owned())];
        let mut flow = PkceFlow::new(cfg);
        let url = flow.start(&[]).await.unwrap().auth_url;
        let redirect_pos = url.find("redirect=127.0.0.1").unwrap();
        let redirect_uri_pos = url.find("redirect_uri=").unwrap();
        assert!(redirect_pos < redirect_uri_pos);
    }

    #[tokio::test]
    async fn authorize_url_appends_trailing_params_from_config_and_call_site() {
        let mut cfg = config(
            "https://example.com/authorize".to_owned(),
            "https://example.com/token".to_owned(),
        );
        cfg.authorize_trailing_params = vec![("access_type".to_owned(), "offline".to_owned())];
        let mut flow = PkceFlow::new(cfg);
        let url = flow.start(&[("prompt", "consent")]).await.unwrap().auth_url;
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        let access_type_pos = url.find("access_type=offline").unwrap();
        let prompt_pos = url.find("prompt=consent").unwrap();
        assert!(access_type_pos < prompt_pos);
    }

    async fn spawn_callback(redirect_uri: &str, state: &str, code: &str) {
        let url = format!("{redirect_uri}?code={code}&state={state}");
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let _ = reqwest::Client::new().get(&url).send().await;
        });
    }

    #[tokio::test]
    async fn exchange_sends_pkce_form_with_secret_when_configured_and_parses_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=authorization_code"))
            .and(body_string_contains("code_verifier="))
            .and(body_string_contains("client_secret=shh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "access_abc",
                "refresh_token": "refresh_xyz",
                "expires_in": 3600,
            })))
            .mount(&server)
            .await;

        let mut cfg = config(
            format!("{}/authorize", server.uri()),
            format!("{}/token", server.uri()),
        );
        cfg.client_secret = Some("shh".to_owned());
        let mut flow = PkceFlow::new(cfg);
        let auth_url = flow.start(&[]).await.unwrap().auth_url;
        let redirect_uri = auth_url
            .split("redirect_uri=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        let redirect_uri = urlencoding_decode(&redirect_uri);
        let state = auth_url
            .split("state=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        spawn_callback(&redirect_uri, &state, "auth_code_xyz").await;

        let token = flow.exchange(Duration::from_secs(2)).await.unwrap();
        assert_eq!(token.access_token, "access_abc");
        assert_eq!(token.refresh_token.as_deref(), Some("refresh_xyz"));
        assert_eq!(token.expires_in, Some(3600));
    }

    fn urlencoding_decode(s: &str) -> String {
        s.replace("%3A", ":")
            .replace("%2F", "/")
            .replace("%2C", ",")
    }

    #[tokio::test]
    async fn exchange_propagates_http_error_on_4xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"})),
            )
            .mount(&server)
            .await;

        let mut flow = PkceFlow::new(config(
            format!("{}/authorize", server.uri()),
            format!("{}/token", server.uri()),
        ));
        let auth_url = flow.start(&[]).await.unwrap().auth_url;
        let redirect_uri = auth_url
            .split("redirect_uri=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        let redirect_uri = urlencoding_decode(&redirect_uri);
        let state = auth_url
            .split("state=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        spawn_callback(&redirect_uri, &state, "bad_code").await;

        let err = flow.exchange(Duration::from_secs(2)).await.unwrap_err();
        assert!(matches!(err, PlatformError::Http { status: 400, .. }));
    }

    #[tokio::test]
    async fn exchange_response_missing_refresh_token_and_expires_in_deserializes_to_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "access_only",
            })))
            .mount(&server)
            .await;

        let mut flow = PkceFlow::new(config(
            format!("{}/authorize", server.uri()),
            format!("{}/token", server.uri()),
        ));
        let auth_url = flow.start(&[]).await.unwrap().auth_url;
        let redirect_uri = auth_url
            .split("redirect_uri=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        let redirect_uri = urlencoding_decode(&redirect_uri);
        let state = auth_url
            .split("state=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();
        spawn_callback(&redirect_uri, &state, "auth_code_abc").await;

        let token = flow.exchange(Duration::from_secs(2)).await.unwrap();
        assert_eq!(token.access_token, "access_only");
        assert!(token.refresh_token.is_none());
        assert!(token.expires_in.is_none());
    }

    #[tokio::test]
    async fn refresh_any_client_error_policy_maps_400_and_401_to_reauth() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;

        let refresher = PkceRefresher::new(PkceRefreshConfig {
            platform: "kick".to_owned(),
            client_id: "cid".to_owned(),
            client_secret: None,
            token_endpoint: format!("{}/token", server.uri()),
            reauth_policy: ReauthPolicy::AnyClientError,
        });
        let err = refresher.refresh("expired").await.unwrap_err();
        assert!(matches!(err, PlatformError::ReauthRequired { platform } if platform == "kick"));
    }

    #[tokio::test]
    async fn refresh_invalid_grant_on_400_policy_ignores_401() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
            .mount(&server)
            .await;

        let refresher = PkceRefresher::new(PkceRefreshConfig {
            platform: "youtube".to_owned(),
            client_id: "cid".to_owned(),
            client_secret: Some("secret".to_owned()),
            token_endpoint: format!("{}/token", server.uri()),
            reauth_policy: ReauthPolicy::InvalidGrantOn400,
        });
        let err = refresher.refresh("expired").await.unwrap_err();
        assert!(matches!(err, PlatformError::Http { status: 401, .. }));
    }

    #[tokio::test]
    async fn refresh_invalid_grant_on_400_policy_matches_400_with_invalid_grant_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(400).set_body_json(json!({"error": "invalid_grant"})),
            )
            .mount(&server)
            .await;

        let refresher = PkceRefresher::new(PkceRefreshConfig {
            platform: "youtube".to_owned(),
            client_id: "cid".to_owned(),
            client_secret: Some("secret".to_owned()),
            token_endpoint: format!("{}/token", server.uri()),
            reauth_policy: ReauthPolicy::InvalidGrantOn400,
        });
        let err = refresher.refresh("expired").await.unwrap_err();
        assert!(matches!(err, PlatformError::ReauthRequired { platform } if platform == "youtube"));
    }

    #[tokio::test]
    async fn refresh_sends_client_secret_only_when_configured() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("client_secret=shh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new_access",
                "expires_in": 3600,
            })))
            .mount(&server)
            .await;

        let refresher = PkceRefresher::new(PkceRefreshConfig {
            platform: "youtube".to_owned(),
            client_id: "cid".to_owned(),
            client_secret: Some("shh".to_owned()),
            token_endpoint: format!("{}/token", server.uri()),
            reauth_policy: ReauthPolicy::InvalidGrantOn400,
        });
        refresher.refresh("rt").await.unwrap();

        let reqs = server.received_requests().await.unwrap();
        assert_eq!(reqs.len(), 1);
        let body = std::str::from_utf8(&reqs[0].body).unwrap();
        assert!(body.contains("client_secret=shh"));
    }

    #[tokio::test]
    async fn refresh_omits_client_secret_when_none() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "new_access",
                "expires_in": 3600,
            })))
            .mount(&server)
            .await;

        let refresher = PkceRefresher::new(PkceRefreshConfig {
            platform: "twitch".to_owned(),
            client_id: "cid".to_owned(),
            client_secret: None,
            token_endpoint: format!("{}/token", server.uri()),
            reauth_policy: ReauthPolicy::AnyClientError,
        });
        refresher.refresh("rt").await.unwrap();

        let reqs = server.received_requests().await.unwrap();
        let body = std::str::from_utf8(&reqs[0].body).unwrap();
        assert!(!body.contains("client_secret"));
    }

    #[tokio::test]
    async fn refresh_network_error_strips_url() {
        let refresher = PkceRefresher::new(PkceRefreshConfig {
            platform: "twitch".to_owned(),
            client_id: "cid".to_owned(),
            client_secret: None,
            token_endpoint: "https://192.0.2.1/token".to_owned(),
            reauth_policy: ReauthPolicy::AnyClientError,
        });
        let err = refresher.refresh("rt").await.unwrap_err();
        assert!(!format!("{err}").contains("192.0.2.1"));
    }
}
