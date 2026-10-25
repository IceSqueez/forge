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
mod tests {
    use super::*;
    use forge_net::InvalidRequest;
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

    fn fresh_counter() -> Arc<AtomicU32> {
        Arc::new(AtomicU32::new(0))
    }

    fn validate_url(client: &ScriptHttpClient, url: &str) -> Result<(), HttpError> {
        client.validate(url, &fresh_counter()).map(|_| ())
    }

    async fn answering(template: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(template)
            .mount(&server)
            .await;
        server
    }

    async fn in_blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        tokio::task::spawn_blocking(work).await.unwrap()
    }

    #[test]
    fn get_http_url_returns_https_required() {
        let client = ScriptHttpClient::new(allowlisted(&["example.com"])).unwrap();
        assert!(matches!(
            client.get("http://example.com/path", &fresh_counter()),
            Err(HttpError::HttpsRequired)
        ));
    }

    #[test]
    fn get_unallowed_domain_returns_domain_not_allowed() {
        let client = ScriptHttpClient::new(allowlisted(&["example.com"])).unwrap();
        assert!(matches!(
            client.get("https://other.com/", &fresh_counter()),
            Err(HttpError::DomainNotAllowed)
        ));
    }

    #[test]
    fn empty_allowlist_denies_all_domains() {
        let client = ScriptHttpClient::new(allowlisted(&[])).unwrap();
        assert!(matches!(
            client.get("https://example.com/", &fresh_counter()),
            Err(HttpError::DomainNotAllowed)
        ));
    }

    #[test]
    fn wildcard_entry_allows_the_apex_and_subdomains_but_not_lookalikes() {
        let client = ScriptHttpClient::new(allowlisted(&["*.Example.com"])).unwrap();
        for (url, allowed) in [
            ("https://example.com/", true),
            ("https://api.EXAMPLE.com/", true),
            ("https://a.b.example.com/", true),
            ("https://badexample.com/", false),
            ("https://example.com.evil.net/", false),
        ] {
            let outcome = validate_url(&client, url);
            assert_eq!(
                !matches!(outcome, Err(HttpError::DomainNotAllowed)),
                allowed,
                "{url}: {outcome:?}"
            );
        }
    }

    #[test]
    fn the_call_limit_refuses_at_the_maximum_and_admits_one_below_it() {
        let config = Arc::new(ScriptHttpConfig {
            max_calls_per_script: 3,
            allowed_domains: vec!["example.com".into()],
            ..ScriptHttpConfig::default()
        });
        let client = ScriptHttpClient::new(config).unwrap();
        for (used, refused) in [(2, false), (3, true), (4, true)] {
            let counter = Arc::new(AtomicU32::new(used));
            let outcome = client.validate("https://example.com/", &counter);
            assert_eq!(
                matches!(outcome, Err(HttpError::RateLimitExceeded)),
                refused,
                "{used} used"
            );
        }
    }

    #[test]
    fn a_refused_call_does_not_consume_a_call_slot() {
        let config = Arc::new(ScriptHttpConfig {
            max_calls_per_script: 1,
            allowed_domains: vec!["example.com".into(), "10.0.0.1".into()],
            ..ScriptHttpConfig::default()
        });
        let client = ScriptHttpClient::new(config).unwrap();
        let counter = fresh_counter();
        for url in [
            "http://example.com/",
            "https://other.com/",
            "https://10.0.0.1/",
            "https://user:pw@example.com/",
            "not a url",
        ] {
            assert!(client.get(url, &counter).is_err(), "{url}");
            assert_eq!(counter.load(Ordering::SeqCst), 0, "{url}");
        }
    }

    #[test]
    fn malformed_or_userinfo_urls_fail_with_a_text_that_omits_the_url() {
        let client = ScriptHttpClient::new(allowlisted(&["example.com"])).unwrap();
        for (url, reason) in [
            ("not a url", InvalidRequest::MalformedUrl),
            ("", InvalidRequest::MalformedUrl),
            (
                "https://user:hunter2@example.com/",
                InvalidRequest::EmbeddedCredentials,
            ),
        ] {
            match client.get(url, &fresh_counter()).err() {
                Some(HttpError::Network(message)) => {
                    assert_eq!(message, reason.to_string(), "{url}");
                    assert!(!message.contains("hunter2"), "{message}");
                }
                other => panic!("{url}: expected a network error, got {other:?}"),
            }
        }
    }

    #[test]
    fn imds_url_blocked_when_allow_local_false() {
        let config = Arc::new(ScriptHttpConfig {
            allowed_domains: vec!["169.254.169.254".into()],
            allow_local: false,
            ..ScriptHttpConfig::default()
        });
        let client = ScriptHttpClient::new(config).unwrap();
        assert!(matches!(
            client.get(
                "https://169.254.169.254/latest/meta-data/",
                &fresh_counter()
            ),
            Err(HttpError::PrivateAddress)
        ));
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
    fn mapped_and_nat64_loopback_literals_are_blocked_when_allow_local_false() {
        for host in ["[::ffff:7f00:1]", "[64:ff9b::7f00:1]"] {
            let client = ScriptHttpClient::new(allowlisted(&[host])).unwrap();
            assert!(
                matches!(
                    validate_url(&client, &format!("https://{host}/")),
                    Err(HttpError::PrivateAddress)
                ),
                "{host}"
            );
        }
    }

    #[test]
    fn a_call_outside_any_async_runtime_is_a_network_error_that_keeps_the_slot() {
        let client = ScriptHttpClient::new(allowlisted(&["example.com"])).unwrap();
        let counter = fresh_counter();

        let outcome = client.get("https://example.com/", &counter).err();

        assert!(
            matches!(&outcome, Some(HttpError::Network(message)) if message == NO_RUNTIME),
            "{outcome:?}"
        );
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn https_localhost_is_blocked_at_resolution_when_allow_local_false() {
        let outcome = in_blocking(|| {
            let client = ScriptHttpClient::new(allowlisted(&["localhost"])).unwrap();
            client.get("https://localhost:9/", &fresh_counter()).err()
        })
        .await;

        assert!(
            matches!(outcome, Some(HttpError::PrivateAddress)),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn get_localhost_name_over_plain_http_returns_200_with_allow_local() {
        let server = answering(ResponseTemplate::new(200).set_body_string("hello")).await;
        let url = format!("http://localhost:{}/test", server.address().port());

        let result = in_blocking(move || {
            let client = ScriptHttpClient::new(allowlisted_local(&["localhost"])).unwrap();
            client.get(&url, &fresh_counter())
        })
        .await;

        assert_eq!(result.unwrap().status, 200);
    }

    #[tokio::test]
    async fn get_returns_status_body_headers_and_a_query_free_normalized_url() {
        let server = answering(
            ResponseTemplate::new(200)
                .insert_header("x-trace", "t-1")
                .set_body_string("hello"),
        )
        .await;
        let host = server.address().ip().to_string();
        let url = format!("{}/test?token=QUERY-SECRET", server.uri());
        let config = allowlisted_local(&[host.as_str()]);

        let response = in_blocking(move || {
            let client = ScriptHttpClient::new(config).unwrap();
            client.get(&url, &fresh_counter())
        })
        .await
        .unwrap();

        assert_eq!(
            (
                response.status,
                response.body.as_str(),
                response.truncated,
                response.headers.get("x-trace").map(String::as_str),
                response.url_normalized,
            ),
            (
                200,
                "hello",
                false,
                Some("t-1"),
                format!("http://{host}/test")
            )
        );
    }

    #[tokio::test]
    async fn error_statuses_come_back_as_responses_and_redirects_are_not_followed() {
        for (template, status, body) in [
            (
                ResponseTemplate::new(404).set_body_string("missing"),
                404,
                "missing",
            ),
            (
                ResponseTemplate::new(503).set_body_string("down"),
                503,
                "down",
            ),
            (
                ResponseTemplate::new(302)
                    .insert_header("location", "/elsewhere")
                    .set_body_string("moved"),
                302,
                "moved",
            ),
        ] {
            let server = answering(template).await;
            let host = server.address().ip().to_string();
            let url = format!("{}/x", server.uri());
            let config = allowlisted_local(&[host.as_str()]);

            let response = in_blocking(move || {
                let client = ScriptHttpClient::new(config).unwrap();
                client.get(&url, &fresh_counter())
            })
            .await
            .unwrap();

            assert_eq!((response.status, response.body.as_str()), (status, body));
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn post_sends_the_body_without_a_content_type() {
        let server = answering(ResponseTemplate::new(201)).await;
        let host = server.address().ip().to_string();
        let url = format!("{}/p", server.uri());
        let config = allowlisted_local(&[host.as_str()]);

        let response = in_blocking(move || {
            let client = ScriptHttpClient::new(config).unwrap();
            client.post(&url, "payload-7", &fresh_counter())
        })
        .await
        .unwrap();

        let seen = server.received_requests().await.unwrap();
        assert_eq!(
            (
                response.status,
                seen[0].body.as_slice(),
                seen[0].headers.get("content-type").is_some()
            ),
            (201, &b"payload-7"[..], false)
        );
    }

    #[tokio::test]
    async fn a_sent_call_consumes_a_slot_even_when_the_transport_fails() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let config = Arc::new(ScriptHttpConfig {
            allowed_domains: vec!["127.0.0.1".into()],
            allow_local: true,
            max_calls_per_script: 1,
            ..ScriptHttpConfig::default()
        });

        let (first, second) = in_blocking(move || {
            let client = ScriptHttpClient::new(config).unwrap();
            let counter = fresh_counter();
            let url = format!("http://127.0.0.1:{port}/");
            (
                client.get(&url, &counter).err(),
                client.get(&url, &counter).err(),
            )
        })
        .await;

        assert!(matches!(first, Some(HttpError::Network(_))), "{first:?}");
        assert!(
            matches!(second, Some(HttpError::RateLimitExceeded)),
            "{second:?}"
        );
    }

    #[tokio::test]
    async fn a_response_that_outlasts_the_timeout_is_a_timeout() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
            }
        });
        let config = Arc::new(ScriptHttpConfig {
            allowed_domains: vec!["127.0.0.1".into()],
            allow_local: true,
            timeout_ms: 200,
            ..ScriptHttpConfig::default()
        });

        let outcome = in_blocking(move || {
            let client = ScriptHttpClient::new(config).unwrap();
            client
                .get(&format!("http://127.0.0.1:{port}/"), &fresh_counter())
                .err()
        })
        .await;

        assert!(matches!(outcome, Some(HttpError::Timeout)), "{outcome:?}");
    }

    #[tokio::test]
    async fn response_is_truncated_only_past_the_configured_cap() {
        for (length, truncated) in [(1024, false), (1025, true), (2048, true)] {
            let server =
                answering(ResponseTemplate::new(200).set_body_string("x".repeat(length))).await;
            let host = server.address().ip().to_string();
            let config = Arc::new(ScriptHttpConfig {
                allowed_domains: vec![host],
                allow_local: true,
                max_response_bytes: 1024,
                timeout_ms: 30_000,
                ..ScriptHttpConfig::default()
            });
            let url = format!("{}/big", server.uri());

            let response = in_blocking(move || {
                let client = ScriptHttpClient::new(config).unwrap();
                client.get(&url, &fresh_counter())
            })
            .await
            .unwrap();

            assert_eq!(
                (response.truncated, response.body.len()),
                (truncated, 1024),
                "{length}"
            );
        }
    }
}
