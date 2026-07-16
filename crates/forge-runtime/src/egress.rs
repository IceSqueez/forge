use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

use forge_platform_core::is_private_or_special;
use tokio::sync::Semaphore;

const MAX_RESPONSE_BYTES: usize = 1_048_576;
const MAX_REDIRECT_HOPS: u32 = 10;
const MAX_CONCURRENT_REQUESTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

pub struct EgressRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub query_params: BTreeMap<String, String>,
    pub body: Option<String>,
    pub content_type: Option<String>,
    pub timeout: Duration,
    pub follow_redirects: bool,
    /// When false (default) the SSRF denylist rejects loopback/private/link-local
    /// targets; when true the denylist is bypassed so LAN/localhost URLs reach the
    /// network. Read per-execution from settings so the toggle is hot.
    pub allow_local: bool,
}

pub struct EgressResponse {
    pub status: u16,
    pub body: String,
    pub headers: BTreeMap<String, String>,
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum EgressError {
    #[error("invalid url")]
    InvalidUrl,
    #[error("only http and https urls are allowed")]
    SchemeNotAllowed,
    #[error("blocked: target resolves to a private or local address")]
    BlockedAddress,
    #[error("host did not resolve")]
    UnresolvableHost,
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("request timed out")]
    Timeout,
    #[error("network error: {0}")]
    Network(String),
}

/// Cross-cutting HTTP egress for `core.http.*` sub-actions. Owns its own SSRF
/// denylist and per-request budget instead of routing through the platform
/// `TokenBucketRateLimiter`: that limiter's buckets are keyed to documented
/// per-platform API quotas, and an arbitrary user-authored URL maps to no
/// platform bucket. The global semaphore caps concurrent in-flight egress
/// requests across the whole runtime.
pub struct EgressClient {
    client: reqwest::Client,
    concurrency: Semaphore,
}

impl EgressClient {
    pub fn new() -> Result<Self, EgressError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("forge/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| EgressError::Network(e.without_url().to_string()))?;
        Ok(Self {
            client,
            concurrency: Semaphore::new(MAX_CONCURRENT_REQUESTS),
        })
    }

    /// Validates the URL against the SSRF denylist, sends it, and follows
    /// redirects manually so every hop is re-validated against the same
    /// classifier. The host's resolved IPs are re-checked post-resolution before
    /// each request leaves - a literal IP or any DNS result inside private space
    /// is rejected unless `allow_local` is set.
    pub async fn send(&self, req: EgressRequest) -> Result<EgressResponse, EgressError> {
        let _permit = self
            .concurrency
            .acquire()
            .await
            .map_err(|_| EgressError::Network("egress pool closed".to_owned()))?;

        let mut url = reqwest::Url::parse(&req.url).map_err(|_| EgressError::InvalidUrl)?;
        if !req.query_params.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in &req.query_params {
                pairs.append_pair(k, v);
            }
        }

        let mut method = req.method;
        let mut body = req.body.clone();
        let mut hops = 0u32;

        loop {
            validate_url(&url, req.allow_local).await?;
            let response = self.dispatch(&url, method, &req, body.as_deref()).await?;
            let status = response.status();

            if req.follow_redirects && status.is_redirection() {
                if hops >= MAX_REDIRECT_HOPS {
                    return Err(EgressError::TooManyRedirects);
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok());
                let Some(location) = location else {
                    return read_response(response).await;
                };
                let next = url.join(location).map_err(|_| EgressError::InvalidUrl)?;
                if matches!(status.as_u16(), 301..=303) {
                    method = HttpMethod::Get;
                    body = None;
                }
                url = next;
                hops += 1;
                continue;
            }

            return read_response(response).await;
        }
    }

    async fn dispatch(
        &self,
        url: &reqwest::Url,
        method: HttpMethod,
        req: &EgressRequest,
        body: Option<&str>,
    ) -> Result<reqwest::Response, EgressError> {
        let mut builder = self
            .client
            .request(to_reqwest_method(method), url.clone())
            .timeout(req.timeout);
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }
        if let Some(content_type) = &req.content_type {
            builder = builder.header(reqwest::header::CONTENT_TYPE, content_type);
        }
        if let Some(body) = body {
            builder = builder.body(body.to_owned());
        }
        builder.send().await.map_err(map_send_error)
    }
}

async fn validate_url(url: &reqwest::Url, allow_local: bool) -> Result<(), EgressError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(EgressError::SchemeNotAllowed),
    }
    if allow_local {
        return Ok(());
    }
    let host = url.host_str().ok_or(EgressError::InvalidUrl)?;
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_or_special(ip) {
            return Err(EgressError::BlockedAddress);
        }
        return Ok(());
    }
    let port = url.port_or_known_default().unwrap_or(443);
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| EgressError::UnresolvableHost)?;
    let mut any = false;
    for addr in resolved {
        any = true;
        if is_private_or_special(addr.ip()) {
            return Err(EgressError::BlockedAddress);
        }
    }
    if any {
        Ok(())
    } else {
        Err(EgressError::UnresolvableHost)
    }
}

async fn read_response(response: reqwest::Response) -> Result<EgressResponse, EgressError> {
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_owned(), v.to_owned()))
        })
        .collect();

    let mut response = response;
    let mut buf: Vec<u8> = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = response.chunk().await.map_err(map_send_error)? {
        if buf.len() + chunk.len() > MAX_RESPONSE_BYTES {
            let remaining = MAX_RESPONSE_BYTES - buf.len();
            buf.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        buf.extend_from_slice(&chunk);
    }

    Ok(EgressResponse {
        status,
        body: String::from_utf8_lossy(&buf).into_owned(),
        headers,
        truncated,
    })
}

fn map_send_error(e: reqwest::Error) -> EgressError {
    if e.is_timeout() {
        EgressError::Timeout
    } else {
        EgressError::Network(e.without_url().to_string())
    }
}

fn to_reqwest_method(method: HttpMethod) -> reqwest::Method {
    match method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Instant;
    use wiremock::matchers::{body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // wiremock binds an in-process HTTP server on 127.0.0.1 (a mock backend on
    // loopback - never a real external service). Every SSRF-reject case asserts
    // the request never left the runtime by checking the mock recorded zero hits.

    // `EgressResponse` has no Debug derive, so `Result::unwrap_err` is unavailable
    // on the send result; extract the error by match instead.
    fn expect_err(result: Result<EgressResponse, EgressError>) -> EgressError {
        match result {
            Ok(_) => panic!("expected an egress error, got an Ok response"),
            Err(e) => e,
        }
    }

    fn request(url: String, allow_local: bool) -> EgressRequest {
        EgressRequest {
            method: HttpMethod::Get,
            url,
            headers: BTreeMap::new(),
            query_params: BTreeMap::new(),
            body: None,
            content_type: None,
            timeout: Duration::from_secs(5),
            follow_redirects: false,
            allow_local,
        }
    }

    #[tokio::test]
    async fn loopback_target_is_rejected_before_request_when_allow_local_false() {
        let server = MockServer::start().await;
        // A catch-all that, if ever hit, would let the request through.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();

        let err = expect_err(client.send(request(server.uri(), false)).await);

        assert!(matches!(err, EgressError::BlockedAddress));
        // Load-bearing: the denylist fired BEFORE any byte left - the loopback
        // mock saw nothing.
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn same_loopback_target_is_allowed_when_allow_local_true() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/ping"))
            .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();

        let resp = client
            .send(request(format!("{}/ping", server.uri()), true))
            .await
            .unwrap();

        // The toggle is the ONLY difference from the reject case above.
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "pong");
    }

    #[tokio::test]
    async fn literal_private_ip_is_rejected() {
        let client = EgressClient::new().unwrap();
        let err = expect_err(
            client
                .send(request("http://10.0.0.1/admin".to_owned(), false))
                .await,
        );
        assert!(matches!(err, EgressError::BlockedAddress));
    }

    #[tokio::test]
    async fn cloud_metadata_ip_is_rejected() {
        let client = EgressClient::new().unwrap();
        let err = expect_err(
            client
                .send(request(
                    "http://169.254.169.254/latest/meta-data/".to_owned(),
                    false,
                ))
                .await,
        );
        assert!(matches!(err, EgressError::BlockedAddress));
    }

    #[tokio::test]
    async fn localhost_hostname_is_rejected_after_dns_resolution() {
        // `localhost` is a name, not a literal IP, so this exercises the
        // post-resolution branch: the resolved 127.0.0.1 must still be blocked.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let port = server.address().port();
        let client = EgressClient::new().unwrap();

        let err = expect_err(
            client
                .send(request(format!("http://localhost:{port}/"), false))
                .await,
        );

        assert!(matches!(err, EgressError::BlockedAddress));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn non_http_scheme_is_rejected() {
        let client = EgressClient::new().unwrap();
        // allow_local is irrelevant: the scheme guard runs before the toggle.
        let err = expect_err(
            client
                .send(request("ftp://example.com/file".to_owned(), true))
                .await,
        );
        assert!(matches!(err, EgressError::SchemeNotAllowed));
    }

    #[tokio::test]
    async fn redirect_to_disallowed_scheme_is_revalidated_per_hop() {
        // The first hop (loopback wiremock, allow_local) passes; the 302 target is
        // a non-http scheme. If per-hop revalidation were skipped the client would
        // try to follow blindly - instead the scheme guard must reject the new URL.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", "ftp://evil.example/"),
            )
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();
        let mut req = request(server.uri(), true);
        req.follow_redirects = true;

        let err = expect_err(client.send(req).await);
        assert!(matches!(err, EgressError::SchemeNotAllowed));
    }

    #[tokio::test]
    async fn redirect_to_allowed_target_is_followed_to_final_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/dest"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/dest"))
            .respond_with(ResponseTemplate::new(200).set_body_string("arrived"))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();
        let mut req = request(format!("{}/start", server.uri()), true);
        req.follow_redirects = true;

        let resp = client.send(req).await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "arrived");
    }

    #[tokio::test]
    async fn redirect_loop_past_hop_cap_returns_too_many_redirects() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/loop"))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();
        let mut req = request(format!("{}/loop", server.uri()), true);
        req.follow_redirects = true;

        let err = expect_err(client.send(req).await);
        assert!(matches!(err, EgressError::TooManyRedirects));
    }

    #[tokio::test]
    async fn oversized_body_is_truncated_at_the_cap() {
        let big = "x".repeat(MAX_RESPONSE_BYTES + 4096);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string(big))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();

        let resp = client.send(request(server.uri(), true)).await.unwrap();
        assert!(resp.truncated);
        assert_eq!(resp.body.len(), MAX_RESPONSE_BYTES);
    }

    #[tokio::test]
    async fn response_slower_than_timeout_fails_promptly() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();
        let mut req = request(server.uri(), true);
        req.timeout = Duration::from_millis(100);

        let start = Instant::now();
        let err = expect_err(client.send(req).await);
        let elapsed = start.elapsed();

        assert!(matches!(err, EgressError::Timeout));
        // Bounded: the configured 100ms timeout fired, not the 30s mock delay.
        assert!(elapsed < Duration::from_secs(5), "took {elapsed:?}");
    }

    #[tokio::test]
    async fn get_captures_status_body_and_headers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-custom", "marker")
                    .set_body_string("hello"),
            )
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();

        let resp = client.send(request(server.uri(), true)).await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, "hello");
        assert_eq!(
            resp.headers.get("x-custom").map(String::as_str),
            Some("marker")
        );
    }

    #[tokio::test]
    async fn post_marshals_method_and_body() {
        let server = MockServer::start().await;
        // The body matcher means a 201 is returned ONLY if the request actually
        // arrived as a POST carrying the exact body.
        Mock::given(method("POST"))
            .and(body_string("payload-123"))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();
        let mut req = request(server.uri(), true);
        req.method = HttpMethod::Post;
        req.body = Some("payload-123".to_owned());

        let resp = client.send(req).await.unwrap();
        assert_eq!(resp.status, 201);
    }
}
