use std::time::Duration;

use crate::error::PlatformError;

use super::local_callback::LocalCallbackDriver;

pub const REFRESH_BUFFER_SECS: u64 = 5 * 60;

#[derive(Debug, Clone)]
pub struct PkceAuthorizeUrl {
    pub auth_url: String,
}

#[derive(Debug, Clone)]
pub struct PkceAuthorizeConfig {
    pub client_id: String,
    pub authorize_endpoint: String,
    pub scopes: Vec<String>,
    pub authorize_pre_redirect_params: Vec<(String, String)>,
    pub authorize_trailing_params: Vec<(String, String)>,
    pub preferred_port: Option<u16>,
}

pub struct PkceAuthorization {
    pub code: String,
    pub redirect_uri: String,
    pub code_verifier: String,
}

pub struct PkceLoopback {
    config: PkceAuthorizeConfig,
    pending: Option<LocalCallbackDriver>,
}

impl PkceLoopback {
    pub fn new(config: PkceAuthorizeConfig) -> Self {
        Self {
            config,
            pending: None,
        }
    }

    pub async fn start(
        &mut self,
        extra_trailing_params: &[(&str, &str)],
    ) -> Result<PkceAuthorizeUrl, PlatformError> {
        let driver = LocalCallbackDriver::bind(self.config.preferred_port).await?;
        let auth_url = build_authorize_url(&self.config, &driver, extra_trailing_params)?;
        self.pending = Some(driver);
        Ok(PkceAuthorizeUrl { auth_url })
    }

    pub async fn await_authorization(
        &mut self,
        timeout: Duration,
    ) -> Result<PkceAuthorization, PlatformError> {
        let driver = self.pending.take().ok_or_else(|| PlatformError::Auth {
            reason: "exchange called before start".into(),
        })?;
        let redirect_uri = driver.redirect_uri().to_owned();
        let code_verifier = driver.code_verifier().to_owned();
        let callback = driver.await_callback(timeout).await?;
        Ok(PkceAuthorization {
            code: callback.code,
            redirect_uri,
            code_verifier,
        })
    }
}

fn build_authorize_url(
    config: &PkceAuthorizeConfig,
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

    let url = url::Url::parse_with_params(&config.authorize_endpoint, &params).map_err(|e| {
        PlatformError::Auth {
            reason: format!("invalid authorize endpoint URL: {e}"),
        }
    })?;
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRESH_PORT_ATTEMPTS: usize = 16;
    const AUTHORIZE_ENDPOINT: &str = "https://example.com/authorize";

    fn config() -> PkceAuthorizeConfig {
        PkceAuthorizeConfig {
            client_id: "test_client".to_owned(),
            authorize_endpoint: AUTHORIZE_ENDPOINT.to_owned(),
            scopes: vec!["scope:a".to_owned(), "scope:b".to_owned()],
            authorize_pre_redirect_params: Vec::new(),
            authorize_trailing_params: Vec::new(),
            preferred_port: None,
        }
    }

    async fn authorize_url(config: PkceAuthorizeConfig, extra: &[(&str, &str)]) -> String {
        PkceLoopback::new(config)
            .start(extra)
            .await
            .unwrap()
            .auth_url
    }

    fn query_param(auth_url: &str, key: &str) -> String {
        reqwest::Url::parse(auth_url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == key)
            .unwrap()
            .1
            .into_owned()
    }

    #[tokio::test]
    async fn authorize_url_contains_required_pkce_params_and_no_secret() {
        let url = authorize_url(config(), &[]).await;
        assert!(url.starts_with(AUTHORIZE_ENDPOINT));
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
        let mut cfg = config();
        cfg.authorize_pre_redirect_params = vec![("redirect".to_owned(), "127.0.0.1".to_owned())];
        let url = authorize_url(cfg, &[]).await;
        let redirect_pos = url.find("redirect=127.0.0.1").unwrap();
        let redirect_uri_pos = url.find("redirect_uri=").unwrap();
        assert!(redirect_pos < redirect_uri_pos);
    }

    #[tokio::test]
    async fn authorize_url_appends_trailing_params_from_config_and_call_site() {
        let mut cfg = config();
        cfg.authorize_trailing_params = vec![("access_type".to_owned(), "offline".to_owned())];
        let url = authorize_url(cfg, &[("prompt", "consent")]).await;
        let access_type_pos = url.find("access_type=offline").unwrap();
        let prompt_pos = url.find("prompt=consent").unwrap();
        assert!(access_type_pos < prompt_pos);
    }

    #[tokio::test]
    async fn awaiting_authorization_before_start_is_an_auth_error() {
        let outcome = PkceLoopback::new(config())
            .await_authorization(Duration::from_secs(1))
            .await;
        assert!(matches!(outcome, Err(PlatformError::Auth { .. })));
    }

    #[tokio::test]
    async fn start_binds_the_configured_preferred_port_into_the_redirect_uri() {
        let mut bound = None;
        for _ in 0..FRESH_PORT_ATTEMPTS {
            let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = probe.local_addr().unwrap().port();
            drop(probe);
            let mut cfg = config();
            cfg.preferred_port = Some(port);
            match PkceLoopback::new(cfg).start(&[]).await {
                Err(PlatformError::LoopbackPortInUse { .. }) => {}
                outcome => {
                    bound = Some((port, outcome.unwrap().auth_url));
                    break;
                }
            }
        }
        assert!(
            bound.is_some(),
            "no free port obtained after {FRESH_PORT_ATTEMPTS} attempts"
        );
        let (port, url) = bound.unwrap();

        let redirect_uri = reqwest::Url::parse(&query_param(&url, "redirect_uri")).unwrap();
        assert_eq!(redirect_uri.port(), Some(port));
    }
}
