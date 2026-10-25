use std::collections::BTreeMap;
use std::time::Duration;

use forge_net::{
    Gateway, GatewayFailure, GatewayRequest, GatewayResponse, HostRejection, InvalidRequest,
    Method, RedirectRefusal, RequestBody, ResolvedTarget, TransportKind,
};
use tokio::sync::Semaphore;

const MAX_RESPONSE_BYTES: u64 = 1_048_576;
const MAX_CONCURRENT_REQUESTS: usize = 16;
const ALLOWED_SCHEMES: [&str; 2] = ["http", "https"];
const CONTENT_TYPE_HEADER: &str = "content-type";

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

pub struct EgressClient {
    public_only: Gateway,
    local_allowed: Gateway,
    concurrency: Semaphore,
}

impl EgressClient {
    pub fn new() -> Result<Self, EgressError> {
        let build = |allow_local| {
            Gateway::user_directed(allow_local)
                .map_err(|error| EgressError::Network(error.to_string()))
        };
        Ok(Self {
            public_only: build(false)?,
            local_allowed: build(true)?,
            concurrency: Semaphore::new(MAX_CONCURRENT_REQUESTS),
        })
    }

    pub async fn send(&self, req: EgressRequest) -> Result<EgressResponse, EgressError> {
        let _permit = self
            .concurrency
            .acquire()
            .await
            .map_err(|_| EgressError::Network("egress pool closed".to_owned()))?;

        let target = ResolvedTarget::absolute(&req.url).map_err(invalid_request_error)?;
        if !ALLOWED_SCHEMES.contains(&target.scheme()) {
            return Err(EgressError::SchemeNotAllowed);
        }
        let gateway = if req.allow_local {
            &self.local_allowed
        } else {
            &self.public_only
        };
        let response = gateway
            .send(gateway_request(req))
            .await
            .map_err(failure_error)?;
        Ok(egress_response(response))
    }
}

fn gateway_request(req: EgressRequest) -> GatewayRequest {
    let EgressRequest {
        method,
        url,
        mut headers,
        query_params,
        body,
        content_type,
        timeout,
        follow_redirects,
        allow_local: _,
    } = req;
    let mut request = GatewayRequest::absolute(gateway_method(method), url)
        .any_status_as_data()
        .truncate_body_at_cap()
        .body_cap(MAX_RESPONSE_BYTES)
        .total_budget(timeout)
        .follow_redirects(follow_redirects);
    if let Some(body) = body {
        let content_type = content_type.or_else(|| take_content_type(&mut headers));
        request = request.body(match content_type {
            Some(content_type) => RequestBody::raw(content_type, body),
            None => RequestBody::untyped(body),
        });
    } else if let Some(content_type) = content_type {
        headers.insert(CONTENT_TYPE_HEADER.to_owned(), content_type);
    }
    for (name, value) in headers {
        request = request.user_header(name, value);
    }
    for (name, value) in query_params {
        request = request.query(name, value);
    }
    request
}

fn take_content_type(headers: &mut BTreeMap<String, String>) -> Option<String> {
    let name = headers
        .keys()
        .find(|name| name.eq_ignore_ascii_case(CONTENT_TYPE_HEADER))
        .cloned()?;
    headers.remove(&name)
}

fn egress_response(response: GatewayResponse) -> EgressResponse {
    let headers = response.headers().iter().cloned().collect();
    EgressResponse {
        status: response.status(),
        truncated: response.truncated(),
        headers,
        body: String::from_utf8_lossy(response.body()).into_owned(),
    }
}

fn failure_error(failure: GatewayFailure) -> EgressError {
    match failure {
        GatewayFailure::InvalidRequest(reason) => invalid_request_error(reason),
        GatewayFailure::AddressRefused(HostRejection::PrivateOrSpecial) => {
            EgressError::BlockedAddress
        }
        GatewayFailure::AddressRefused(HostRejection::Unresolvable) => {
            EgressError::UnresolvableHost
        }
        GatewayFailure::Transport(transport) => match transport.redirect_refusal() {
            Some(RedirectRefusal::TooManyHops) => EgressError::TooManyRedirects,
            Some(RedirectRefusal::UnsupportedScheme) => EgressError::SchemeNotAllowed,
            Some(RedirectRefusal::MalformedTarget | RedirectRefusal::EmbeddedCredentials) => {
                EgressError::InvalidUrl
            }
            None if transport.kind() == TransportKind::Timeout => EgressError::Timeout,
            None => EgressError::Network(transport.cause().to_owned()),
        },
        other => EgressError::Network(other.to_string()),
    }
}

fn invalid_request_error(reason: InvalidRequest) -> EgressError {
    match reason {
        InvalidRequest::MalformedUrl | InvalidRequest::EmbeddedCredentials => {
            EgressError::InvalidUrl
        }
        InvalidRequest::UnsupportedScheme => EgressError::SchemeNotAllowed,
        other => EgressError::Network(other.to_string()),
    }
}

fn gateway_method(method: HttpMethod) -> Method {
    match method {
        HttpMethod::Get => Method::Get,
        HttpMethod::Post => Method::Post,
        HttpMethod::Put => Method::Put,
        HttpMethod::Patch => Method::Patch,
        HttpMethod::Delete => Method::Delete,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();

        let err = expect_err(client.send(request(server.uri(), false)).await);

        assert!(matches!(err, EgressError::BlockedAddress));
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

    #[test]
    fn bracketed_public_ipv6_literal_passes_validation() {
        let url = reqwest::Url::parse("http://[2606:4700::1]/").unwrap();
        assert!(validate_url(&url, false).is_ok());
    }

    #[test]
    fn bracketed_ipv6_loopback_literal_is_blocked() {
        let url = reqwest::Url::parse("http://[::1]:8080/").unwrap();
        assert!(matches!(
            validate_url(&url, false),
            Err(EgressError::BlockedAddress)
        ));
    }

    #[test]
    fn ipv4_mapped_loopback_literal_is_blocked() {
        let url = reqwest::Url::parse("http://[::ffff:127.0.0.1]/").unwrap();
        assert!(matches!(
            validate_url(&url, false),
            Err(EgressError::BlockedAddress)
        ));
    }

    #[tokio::test]
    async fn localhost_hostname_is_rejected_after_dns_resolution() {
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
        let err = expect_err(
            client
                .send(request("ftp://example.com/file".to_owned(), true))
                .await,
        );
        assert!(matches!(err, EgressError::SchemeNotAllowed));
    }

    #[tokio::test]
    async fn redirect_to_disallowed_scheme_is_revalidated_per_hop() {
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
    async fn response_slower_than_timeout_returns_timeout() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();
        let mut req = request(server.uri(), true);
        req.timeout = Duration::from_millis(100);

        let err = expect_err(client.send(req).await);

        assert!(matches!(err, EgressError::Timeout));
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

    const USER_HEADERS: [(&str, &str); 3] = [
        ("authorization", "Bearer secret-token"),
        ("cookie", "session=secret-cookie"),
        ("x-api-key", "secret-key"),
    ];

    fn redirecting_request(url: String) -> EgressRequest {
        let mut req = request(url, true);
        req.follow_redirects = true;
        req.headers = USER_HEADERS
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        req
    }

    async fn mount_redirect(server: &MockServer, from: &str, to: String) {
        Mock::given(method("GET"))
            .and(path(from))
            .respond_with(ResponseTemplate::new(302).insert_header("location", to.as_str()))
            .mount(server)
            .await;
    }

    async fn mount_final(server: &MockServer, at: &str) {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_string("final"))
            .mount(server)
            .await;
    }

    async fn user_headers_seen_at(server: &MockServer, at: &str) -> Vec<&'static str> {
        let requests = server.received_requests().await.unwrap();
        let hit = requests
            .iter()
            .find(|r| r.url.path() == at)
            .expect("final hop must reach the server");
        USER_HEADERS
            .iter()
            .filter(|(name, _)| hit.headers.contains_key(*name))
            .map(|(name, _)| *name)
            .collect()
    }

    #[tokio::test]
    async fn cross_origin_redirect_drops_user_headers() {
        let origin = MockServer::start().await;
        let other = MockServer::start().await;
        mount_redirect(&origin, "/start", format!("{}/dest", other.uri())).await;
        mount_final(&other, "/dest").await;
        let client = EgressClient::new().unwrap();

        let resp = client
            .send(redirecting_request(format!("{}/start", origin.uri())))
            .await
            .unwrap();

        assert_eq!(resp.body, "final");
        assert!(user_headers_seen_at(&other, "/dest").await.is_empty());
    }

    #[tokio::test]
    async fn same_origin_redirect_keeps_user_headers() {
        let origin = MockServer::start().await;
        mount_redirect(&origin, "/start", "/dest".to_owned()).await;
        mount_final(&origin, "/dest").await;
        let client = EgressClient::new().unwrap();

        let resp = client
            .send(redirecting_request(format!("{}/start", origin.uri())))
            .await
            .unwrap();

        assert_eq!(resp.body, "final");
        assert_eq!(
            user_headers_seen_at(&origin, "/dest").await,
            vec!["authorization", "cookie", "x-api-key"]
        );
    }

    #[tokio::test]
    async fn redirect_back_to_original_origin_after_cross_origin_hop_still_drops_user_headers() {
        let origin = MockServer::start().await;
        let other = MockServer::start().await;
        mount_redirect(&origin, "/start", format!("{}/bounce", other.uri())).await;
        mount_redirect(&other, "/bounce", format!("{}/dest", origin.uri())).await;
        mount_final(&origin, "/dest").await;
        let client = EgressClient::new().unwrap();

        let resp = client
            .send(redirecting_request(format!("{}/start", origin.uri())))
            .await
            .unwrap();

        assert_eq!(resp.body, "final");
        assert!(user_headers_seen_at(&origin, "/dest").await.is_empty());
    }
}
