use std::collections::BTreeMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use forge_platform_core::is_private_or_special;

use crate::http_config::ScriptHttpConfig;

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("http: domain not allowed")]
    DomainNotAllowed,
    #[error("http: HTTPS required")]
    HttpsRequired,
    #[error("http: local addresses blocked")]
    PrivateAddress,
    #[error("http: rate limit exceeded")]
    RateLimitExceeded,
    #[error("http: timeout")]
    Timeout,
    #[error("http: {0}")]
    Network(String),
}

pub struct HttpResponse {
    pub status: u16,
    pub body: String,
    pub truncated: bool,
    pub headers: BTreeMap<String, String>,
    pub duration_ms: u64,
    pub url_normalized: String,
}

pub struct ScriptHttpClient {
    config: Arc<ScriptHttpConfig>,
    client: reqwest::blocking::Client,
}

impl ScriptHttpClient {
    pub fn new(config: Arc<ScriptHttpConfig>) -> Result<Self, HttpError> {
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(config.timeout_ms as u64))
            .user_agent(concat!("forge/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| HttpError::Network(e.without_url().to_string()))?;
        Ok(Self { config, client })
    }

    /// Rate-limit counter is checked before incrementing; only bumped when all
    /// validation passes and the request is about to be sent.
    pub fn get(&self, url_str: &str, counter: &Arc<AtomicU32>) -> Result<HttpResponse, HttpError> {
        let url = reqwest::Url::parse(url_str).map_err(|e| HttpError::Network(e.to_string()))?;
        self.validate(&url, counter)?;
        counter.fetch_add(1, Ordering::SeqCst);
        let url_normalized = normalize_url(&url);
        let start = Instant::now();
        let response = self.client.get(url).send().map_err(map_send_error)?;
        self.build_response(response, start, url_normalized)
    }

    pub fn post(
        &self,
        url_str: &str,
        body: &str,
        counter: &Arc<AtomicU32>,
    ) -> Result<HttpResponse, HttpError> {
        let url = reqwest::Url::parse(url_str).map_err(|e| HttpError::Network(e.to_string()))?;
        self.validate(&url, counter)?;
        counter.fetch_add(1, Ordering::SeqCst);
        let url_normalized = normalize_url(&url);
        let start = Instant::now();
        let response = self
            .client
            .post(url)
            .body(body.to_string())
            .send()
            .map_err(map_send_error)?;
        self.build_response(response, start, url_normalized)
    }

    fn validate(&self, url: &reqwest::Url, counter: &Arc<AtomicU32>) -> Result<(), HttpError> {
        if counter.load(Ordering::SeqCst) >= self.config.max_calls_per_script {
            return Err(HttpError::RateLimitExceeded);
        }

        if url.scheme() != "https" {
            let host = url.host_str().unwrap_or_default();
            let is_loopback =
                host == "127.0.0.1" || host == "::1" || host.eq_ignore_ascii_case("localhost");
            if !(self.config.allow_local && is_loopback) {
                return Err(HttpError::HttpsRequired);
            }
        }

        let host_str = url.host_str().unwrap_or_default();
        if !domain_allowed(host_str, &self.config.allowed_domains) {
            return Err(HttpError::DomainNotAllowed);
        }

        if !self.config.allow_local {
            check_private_ip(host_str, url.port_or_known_default().unwrap_or(443))?;
        }

        Ok(())
    }

    fn build_response(
        &self,
        response: reqwest::blocking::Response,
        start: Instant,
        url_normalized: String,
    ) -> Result<HttpResponse, HttpError> {
        let status = response.status().as_u16();
        let headers: BTreeMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_string(), v.to_string()))
            })
            .collect();

        use std::io::Read;
        let max = self.config.max_response_bytes as usize;
        let cap = (max as u64).saturating_add(1);
        let mut buf = Vec::new();
        response
            .take(cap)
            .read_to_end(&mut buf)
            .map_err(|e| HttpError::Network(e.to_string()))?;

        let truncated = buf.len() > max;
        if truncated {
            buf.truncate(max);
        }
        let body = String::from_utf8_lossy(&buf).into_owned();

        Ok(HttpResponse {
            status,
            body,
            truncated,
            headers,
            duration_ms: start.elapsed().as_millis() as u64,
            url_normalized,
        })
    }
}

fn map_send_error(e: reqwest::Error) -> HttpError {
    if e.is_timeout() {
        HttpError::Timeout
    } else {
        HttpError::Network(e.without_url().to_string())
    }
}

fn normalize_url(url: &reqwest::Url) -> String {
    format!(
        "{}://{}{}",
        url.scheme(),
        url.host_str().unwrap_or("?"),
        url.path()
    )
}

fn domain_allowed(host: &str, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return false;
    }
    let host_lower = host.to_lowercase();
    allowed.iter().any(|d| {
        let d_lower = d.to_lowercase();
        if let Some(suffix) = d_lower.strip_prefix("*.") {
            host_lower == suffix || host_lower.ends_with(&format!(".{suffix}"))
        } else {
            d_lower == host_lower
        }
    })
}

fn check_private_ip(host: &str, port: u16) -> Result<(), HttpError> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_or_special(ip) {
            return Err(HttpError::PrivateAddress);
        }
    } else if !host.is_empty() {
        let target = format!("{host}:{port}");
        for addr in target
            .to_socket_addrs()
            .map_err(|e| HttpError::Network(e.to_string()))?
        {
            if is_private_or_special(addr.ip()) {
                return Err(HttpError::PrivateAddress);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn new_without_tls_enforcement(
    config: Arc<ScriptHttpConfig>,
) -> Result<ScriptHttpClient, HttpError> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(config.timeout_ms as u64))
        .user_agent(concat!("forge/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| HttpError::Network(e.without_url().to_string()))?;
    Ok(ScriptHttpClient { config, client })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use wiremock::matchers::any;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn allowlisted(domains: &[&str]) -> Arc<ScriptHttpConfig> {
        Arc::new(ScriptHttpConfig {
            allowed_domains: domains.iter().map(|s| s.to_string()).collect(),
            ..ScriptHttpConfig::default()
        })
    }

    fn allowlisted_local(domains: &[&str]) -> Arc<ScriptHttpConfig> {
        Arc::new(ScriptHttpConfig {
            allowed_domains: domains.iter().map(|s| s.to_string()).collect(),
            allow_local: true,
            ..ScriptHttpConfig::default()
        })
    }

    #[test]
    fn get_http_url_returns_https_required() {
        let client = ScriptHttpClient::new(allowlisted(&["example.com"])).unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        assert!(matches!(
            client.get("http://example.com/path", &counter),
            Err(HttpError::HttpsRequired)
        ));
    }

    #[test]
    fn get_unallowed_domain_returns_domain_not_allowed() {
        let client = ScriptHttpClient::new(allowlisted(&["example.com"])).unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        assert!(matches!(
            client.get("https://other.com/", &counter),
            Err(HttpError::DomainNotAllowed)
        ));
    }

    #[test]
    fn empty_allowlist_denies_all_domains() {
        let client = ScriptHttpClient::new(allowlisted(&[])).unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        assert!(matches!(
            client.get("https://example.com/", &counter),
            Err(HttpError::DomainNotAllowed)
        ));
    }

    #[test]
    fn rate_limit_exceeded_after_max_calls() {
        let config = Arc::new(ScriptHttpConfig {
            max_calls_per_script: 3,
            allowed_domains: vec!["example.com".into()],
            ..ScriptHttpConfig::default()
        });
        let client = ScriptHttpClient::new(config).unwrap();
        let counter = Arc::new(AtomicU32::new(3));
        assert!(matches!(
            client.get("https://example.com/", &counter),
            Err(HttpError::RateLimitExceeded)
        ));
    }

    #[test]
    fn imds_url_blocked_when_allow_local_false() {
        // 169.254.169.254 is an IP literal; no DNS resolution is needed.
        // Even when explicitly allowlisted, the private-IP check fires.
        let config = Arc::new(ScriptHttpConfig {
            allowed_domains: vec!["169.254.169.254".into()],
            allow_local: false,
            ..ScriptHttpConfig::default()
        });
        let client = ScriptHttpClient::new(config).unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        assert!(matches!(
            client.get("https://169.254.169.254/latest/meta-data/", &counter),
            Err(HttpError::PrivateAddress)
        ));
    }

    #[tokio::test]
    async fn get_allowlisted_local_server_returns_200() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;

        let server_url = server.uri();
        let parsed = reqwest::Url::parse(&server_url).unwrap();
        let host = parsed.host_str().unwrap().to_string();

        let config = allowlisted_local(&[host.as_str()]);
        let url = format!("{server_url}/test");

        let result = tokio::task::spawn_blocking(move || {
            let client = new_without_tls_enforcement(config).unwrap();
            let counter = Arc::new(AtomicU32::new(0));
            client.get(&url, &counter)
        })
        .await
        .unwrap();

        let response = result.unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "hello");
        assert!(!response.truncated);
    }

    #[tokio::test]
    async fn response_truncated_when_over_cap() {
        let server = MockServer::start().await;
        let big_body: String = "x".repeat(2048);
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string(big_body))
            .mount(&server)
            .await;

        let server_url = server.uri();
        let parsed = reqwest::Url::parse(&server_url).unwrap();
        let host = parsed.host_str().unwrap().to_string();

        let config = Arc::new(ScriptHttpConfig {
            allowed_domains: vec![host],
            allow_local: true,
            max_response_bytes: 1024,
            ..ScriptHttpConfig::default()
        });
        let url = format!("{server_url}/big");

        let result = tokio::task::spawn_blocking(move || {
            let client = new_without_tls_enforcement(config).unwrap();
            let counter = Arc::new(AtomicU32::new(0));
            client.get(&url, &counter)
        })
        .await
        .unwrap();

        let response = result.unwrap();
        assert!(response.truncated);
        assert_eq!(response.body.len(), 1024);
    }
}
