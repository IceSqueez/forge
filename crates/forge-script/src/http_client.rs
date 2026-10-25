use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use forge_net::{
    CallClass, Gateway, GatewayFailure, GatewayRequest, GatewayResponse, HostRejection, Method,
    RequestBody, ResolvedTarget, TransportKind, ensure_public_ip_literal, is_loopback_host,
};
use tokio::runtime::Handle;

use crate::http_config::ScriptHttpConfig;

const HTTPS_SCHEME: &str = "https";
const HTTP_SCHEME: &str = "http";
const WILDCARD_PREFIX: &str = "*.";
const SCRIPT_CALL_CLASS: CallClass = CallClass::Media;
const NO_RUNTIME: &str = "no async runtime is available to run the request";

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
    gateway: Gateway,
}

impl ScriptHttpClient {
    pub fn new(config: Arc<ScriptHttpConfig>) -> Result<Self, HttpError> {
        let gateway = Gateway::user_directed(config.allow_local)
            .map_err(|error| HttpError::Network(error.to_string()))?;
        Ok(Self { config, gateway })
    }

    pub fn get(&self, url_str: &str, counter: &Arc<AtomicU32>) -> Result<HttpResponse, HttpError> {
        self.call(
            GatewayRequest::absolute(Method::Get, url_str),
            url_str,
            counter,
        )
    }

    pub fn post(
        &self,
        url_str: &str,
        body: &str,
        counter: &Arc<AtomicU32>,
    ) -> Result<HttpResponse, HttpError> {
        let request = GatewayRequest::absolute(Method::Post, url_str)
            .body(RequestBody::untyped(body.as_bytes().to_vec()));
        self.call(request, url_str, counter)
    }

    fn call(
        &self,
        request: GatewayRequest,
        url_str: &str,
        counter: &Arc<AtomicU32>,
    ) -> Result<HttpResponse, HttpError> {
        let target = self.validate(url_str, counter)?;
        let runtime =
            Handle::try_current().map_err(|_| HttpError::Network(NO_RUNTIME.to_owned()))?;
        counter.fetch_add(1, Ordering::SeqCst);
        let request = request
            .class(SCRIPT_CALL_CLASS)
            .any_status_as_data()
            .truncate_body_at_cap()
            .body_cap(u64::from(self.config.max_response_bytes))
            .total_budget(Duration::from_millis(u64::from(self.config.timeout_ms)))
            .follow_redirects(false);
        let start = Instant::now();
        let response = runtime
            .block_on(self.gateway.send(request))
            .map_err(failure_error)?;
        Ok(script_response(response, start, normalize_url(&target)))
    }

    fn validate(
        &self,
        url_str: &str,
        counter: &Arc<AtomicU32>,
    ) -> Result<ResolvedTarget, HttpError> {
        if counter.load(Ordering::SeqCst) >= self.config.max_calls_per_script {
            return Err(HttpError::RateLimitExceeded);
        }

        let target = ResolvedTarget::absolute(url_str)
            .map_err(|reason| HttpError::Network(reason.to_string()))?;
        let host = target.host();
        let scheme_permitted = match target.scheme() {
            HTTPS_SCHEME => true,
            HTTP_SCHEME => self.config.allow_local && is_loopback_host(host),
            _ => false,
        };
        if !scheme_permitted {
            return Err(HttpError::HttpsRequired);
        }

        if !domain_allowed(host, &self.config.allowed_domains) {
            return Err(HttpError::DomainNotAllowed);
        }

        if !self.config.allow_local {
            ensure_public_ip_literal(host).map_err(rejection_error)?;
        }

        Ok(target)
    }
}

fn script_response(
    response: GatewayResponse,
    start: Instant,
    url_normalized: String,
) -> HttpResponse {
    HttpResponse {
        status: response.status(),
        truncated: response.truncated(),
        headers: response.headers().iter().cloned().collect(),
        body: String::from_utf8_lossy(response.body()).into_owned(),
        duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
        url_normalized,
    }
}

fn rejection_error(rejection: HostRejection) -> HttpError {
    match rejection {
        HostRejection::PrivateOrSpecial => HttpError::PrivateAddress,
        HostRejection::Unresolvable => HttpError::Network(rejection.to_string()),
    }
}

fn failure_error(failure: GatewayFailure) -> HttpError {
    match failure {
        GatewayFailure::AddressRefused(rejection) => rejection_error(rejection),
        GatewayFailure::Transport(transport) if transport.kind() == TransportKind::Timeout => {
            HttpError::Timeout
        }
        GatewayFailure::Transport(transport) => HttpError::Network(transport.cause().to_owned()),
        other => HttpError::Network(other.to_string()),
    }
}

fn normalize_url(target: &ResolvedTarget) -> String {
    format!("{}://{}{}", target.scheme(), target.host(), target.path())
}

fn domain_allowed(host: &str, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return false;
    }
    let host_lower = host.to_lowercase();
    allowed.iter().any(|d| {
        let d_lower = d.to_lowercase();
        if let Some(suffix) = d_lower.strip_prefix(WILDCARD_PREFIX) {
            host_lower == suffix || host_lower.ends_with(&format!(".{suffix}"))
        } else {
            d_lower == host_lower
        }
    })
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
            timeout_ms: 30_000,
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

    fn validate_url(client: &ScriptHttpClient, url: &str) -> Result<(), HttpError> {
        let counter = Arc::new(AtomicU32::new(0));
        client.validate(&reqwest::Url::parse(url).unwrap(), &counter)
    }

    #[test]
    fn non_http_scheme_to_loopback_returns_https_required_even_with_allow_local() {
        let client = ScriptHttpClient::new(allowlisted_local(&["localhost"])).unwrap();
        assert!(matches!(
            validate_url(&client, "ftp://localhost/file"),
            Err(HttpError::HttpsRequired)
        ));
    }

    #[test]
    fn plain_http_is_permitted_for_each_loopback_host_spelling_with_allow_local() {
        for host in ["localhost", "127.0.0.1", "[::1]"] {
            let client = ScriptHttpClient::new(allowlisted_local(&[host])).unwrap();
            assert!(
                validate_url(&client, &format!("http://{host}:8080/")).is_ok(),
                "plain http to {host} must pass validation"
            );
        }
    }

    #[test]
    fn plain_http_to_non_loopback_host_returns_https_required_with_allow_local() {
        for host in ["10.0.0.1", "[::ffff:127.0.0.1]"] {
            let client = ScriptHttpClient::new(allowlisted_local(&[host])).unwrap();
            assert!(
                matches!(
                    validate_url(&client, &format!("http://{host}/")),
                    Err(HttpError::HttpsRequired)
                ),
                "plain http to {host} must require https"
            );
        }
    }

    #[test]
    fn https_localhost_is_blocked_at_resolution_when_allow_local_false() {
        let client = ScriptHttpClient::new(allowlisted(&["localhost"])).unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        assert!(matches!(
            client.get("https://localhost:9/", &counter),
            Err(HttpError::PrivateAddress)
        ));
    }

    #[test]
    fn ipv4_mapped_loopback_literal_is_blocked_when_allow_local_false() {
        let client = ScriptHttpClient::new(allowlisted(&["[::ffff:7f00:1]"])).unwrap();
        assert!(matches!(
            validate_url(&client, "https://[::ffff:7f00:1]/"),
            Err(HttpError::PrivateAddress)
        ));
    }

    #[tokio::test]
    async fn get_localhost_name_over_plain_http_returns_200_with_allow_local() {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;
        let url = format!("http://localhost:{}/test", server.address().port());
        let config = allowlisted_local(&["localhost"]);

        let result = tokio::task::spawn_blocking(move || {
            let client = ScriptHttpClient::new(config).unwrap();
            let counter = Arc::new(AtomicU32::new(0));
            client.get(&url, &counter)
        })
        .await
        .unwrap();

        assert_eq!(result.unwrap().status, 200);
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
            let client = ScriptHttpClient::new(config).unwrap();
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
            timeout_ms: 30_000,
            ..ScriptHttpConfig::default()
        });
        let url = format!("{server_url}/big");

        let result = tokio::task::spawn_blocking(move || {
            let client = ScriptHttpClient::new(config).unwrap();
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
