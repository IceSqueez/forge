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
pub(crate) enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

pub(crate) struct EgressRequest {
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

pub(crate) struct EgressResponse {
    pub status: u16,
    pub body: String,
    pub headers: BTreeMap<String, String>,
    pub truncated: bool,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum EgressError {
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

pub(crate) struct EgressClient {
    public_only: Gateway,
    local_allowed: Gateway,
    concurrency: Semaphore,
}

impl EgressClient {
    pub(crate) fn new() -> Result<Self, EgressError> {
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

    pub(crate) async fn send(&self, req: EgressRequest) -> Result<EgressResponse, EgressError> {
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
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::watch;
    use wiremock::matchers::{body_string, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const HEADER_END: &[u8] = b"\r\n\r\n";
    const QUEUED_REQUESTS: usize = 2 * MAX_CONCURRENT_REQUESTS;
    const CLOSING_OK: &[u8] =
        b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";

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

    async fn answering(template: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&server)
            .await;
        server
    }

    async fn read_head(stream: &mut tokio::net::TcpStream) {
        let mut seen = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !seen.windows(HEADER_END.len()).any(|w| w == HEADER_END) {
            let read = stream.read(&mut buffer).await.unwrap();
            if read == 0 {
                return;
            }
            seen.extend_from_slice(&buffer[..read]);
        }
    }

    #[tokio::test]
    async fn loopback_target_is_rejected_before_request_when_allow_local_false() {
        let server = answering(ResponseTemplate::new(reqwest::StatusCode::OK)).await;
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
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("pong"))
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
    async fn private_mapped_and_nat64_literals_are_blocked_when_allow_local_false() {
        let client = EgressClient::new().unwrap();
        for url in [
            "http://10.0.0.1/admin",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]:8080/",
            "http://[::ffff:127.0.0.1]/",
            "http://[64:ff9b::a9fe:a9fe]/",
        ] {
            let err = expect_err(client.send(request(url.to_owned(), false)).await);
            assert!(matches!(err, EgressError::BlockedAddress), "{url}: {err:?}");
        }
    }

    #[tokio::test]
    async fn localhost_hostname_is_rejected_after_dns_resolution() {
        let server = answering(ResponseTemplate::new(reqwest::StatusCode::OK)).await;
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
        for url in ["ftp://example.com/file", "file:///etc/passwd"] {
            let err = expect_err(client.send(request(url.to_owned(), true)).await);
            assert!(
                matches!(err, EgressError::SchemeNotAllowed),
                "{url}: {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn userinfo_or_unparseable_urls_are_invalid_and_never_sent() {
        let server = answering(ResponseTemplate::new(reqwest::StatusCode::OK)).await;
        let authority = server.address().to_string();
        let client = EgressClient::new().unwrap();
        for url in [
            format!("http://user:pw@{authority}/"),
            format!("http://user@{authority}/"),
            "not a url".to_owned(),
            String::new(),
        ] {
            let err = expect_err(client.send(request(url.clone(), true)).await);
            assert!(matches!(err, EgressError::InvalidUrl), "{url}: {err:?}");
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn refused_redirect_targets_map_to_their_egress_error() {
        for (location, expected) in [
            ("ftp://evil.example/", "scheme"),
            ("mailto:someone@example.com", "scheme"),
            ("http://user:pw@127.0.0.1/", "invalid"),
            ("http://[::1", "invalid"),
        ] {
            let server = answering(
                ResponseTemplate::new(reqwest::StatusCode::FOUND)
                    .insert_header("location", location),
            )
            .await;
            let client = EgressClient::new().unwrap();
            let mut req = request(server.uri(), true);
            req.follow_redirects = true;

            let err = expect_err(client.send(req).await);

            let mapped = match err {
                EgressError::SchemeNotAllowed => "scheme",
                EgressError::InvalidUrl => "invalid",
                other => panic!("{location}: unexpected {other:?}"),
            };
            assert_eq!(mapped, expected, "{location}");
        }
    }

    #[tokio::test]
    async fn redirect_to_allowed_target_is_followed_to_final_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::FOUND)
                    .insert_header("location", "/dest"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/dest"))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("arrived"))
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
        let server = answering(
            ResponseTemplate::new(reqwest::StatusCode::FOUND).insert_header("location", "/loop"),
        )
        .await;
        let client = EgressClient::new().unwrap();
        let mut req = request(format!("{}/loop", server.uri()), true);
        req.follow_redirects = true;

        let err = expect_err(client.send(req).await);
        assert!(matches!(err, EgressError::TooManyRedirects));
    }

    #[tokio::test]
    async fn error_and_unfollowed_redirect_statuses_come_back_as_responses_with_their_body() {
        for (template, follow, status, body) in [
            (
                ResponseTemplate::new(reqwest::StatusCode::NOT_FOUND).set_body_string("missing"),
                true,
                404,
                "missing",
            ),
            (
                ResponseTemplate::new(reqwest::StatusCode::INTERNAL_SERVER_ERROR)
                    .set_body_string("boom"),
                true,
                500,
                "boom",
            ),
            (
                ResponseTemplate::new(reqwest::StatusCode::TOO_MANY_REQUESTS)
                    .insert_header("retry-after", "30")
                    .set_body_string("slow"),
                true,
                429,
                "slow",
            ),
            (
                ResponseTemplate::new(reqwest::StatusCode::MULTIPLE_CHOICES)
                    .insert_header("location", "/elsewhere")
                    .set_body_string("choices"),
                true,
                300,
                "choices",
            ),
            (
                ResponseTemplate::new(reqwest::StatusCode::FOUND)
                    .insert_header("location", "/elsewhere")
                    .set_body_string("moved"),
                false,
                302,
                "moved",
            ),
        ] {
            let server = answering(template).await;
            let client = EgressClient::new().unwrap();
            let mut req = request(format!("{}/x", server.uri()), true);
            req.follow_redirects = follow;

            let resp = client.send(req).await.unwrap();

            assert_eq!((resp.status, resp.body.as_str()), (status, body));
            assert_eq!(
                server.received_requests().await.unwrap().len(),
                1,
                "{status} must not be followed"
            );
        }
    }

    #[tokio::test]
    async fn body_is_truncated_only_past_the_cap() {
        let cap = usize::try_from(MAX_RESPONSE_BYTES).unwrap();
        for (length, truncated) in [(cap, false), (cap + 1, true), (cap + 4096, true)] {
            let server = answering(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("x".repeat(length)),
            )
            .await;
            let client = EgressClient::new().unwrap();

            let resp = client.send(request(server.uri(), true)).await.unwrap();

            assert_eq!(resp.truncated, truncated, "{length}");
            assert_eq!(resp.body.len(), cap, "{length}");
        }
    }

    async fn holding_server(reply: &'static [u8]) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_head(&mut stream).await;
            stream.write_all(reply).await.unwrap();
            std::future::pending::<()>().await;
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn the_timeout_bounds_both_waiting_for_headers_and_reading_the_body() {
        const TIMEOUT: Duration = Duration::from_millis(10);
        const HANG_GUARD: Duration = Duration::from_secs(30);
        for reply in [
            &b""[..],
            &b"HTTP/1.1 200 OK\r\ncontent-length: 10\r\n\r\nhalf"[..],
        ] {
            let base = holding_server(reply).await;
            let client = EgressClient::new().unwrap();
            let mut req = request(base, true);
            req.timeout = TIMEOUT;

            let result = tokio::time::timeout(HANG_GUARD, client.send(req))
                .await
                .expect("the request timeout never fired");

            assert!(matches!(expect_err(result), EgressError::Timeout));
        }
    }

    #[tokio::test]
    async fn get_captures_status_body_and_headers() {
        let server = answering(
            ResponseTemplate::new(reqwest::StatusCode::OK)
                .insert_header("x-custom", "marker")
                .set_body_string("hello"),
        )
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
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::CREATED))
            .mount(&server)
            .await;
        let client = EgressClient::new().unwrap();
        let mut req = request(server.uri(), true);
        req.method = HttpMethod::Post;
        req.body = Some("payload-123".to_owned());

        let resp = client.send(req).await.unwrap();
        assert_eq!(resp.status, 201);
    }

    #[tokio::test]
    async fn content_type_comes_from_config_then_the_user_header_and_is_sent_once() {
        for (body, configured, user_header, expected) in [
            (
                Some("p"),
                Some("application/json"),
                None,
                vec!["application/json"],
            ),
            (Some("p"), None, Some("text/plain"), vec!["text/plain"]),
            (Some("p"), None, None, vec![]),
            (None, None, Some("text/plain"), vec!["text/plain"]),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
                .mount(&server)
                .await;
            let client = EgressClient::new().unwrap();
            let mut req = request(server.uri(), true);
            req.method = HttpMethod::Post;
            req.body = body.map(str::to_owned);
            req.content_type = configured.map(str::to_owned);
            if let Some(value) = user_header {
                req.headers
                    .insert("Content-Type".to_owned(), value.to_owned());
            }

            client.send(req).await.unwrap();

            let seen = server.received_requests().await.unwrap();
            let sent: Vec<String> = seen[0]
                .headers
                .get_all("content-type")
                .iter()
                .map(|value| value.to_str().unwrap().to_owned())
                .collect();
            assert_eq!(sent, expected, "{body:?} {configured:?} {user_header:?}");
        }
    }

    #[tokio::test]
    async fn at_most_sixteen_requests_are_in_flight_and_the_rest_wait_for_a_slot() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (accepted_tx, mut accepted) = watch::channel(0_usize);
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        {
            let gate = Arc::clone(&gate);
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            tokio::spawn(async move {
                while let Ok((mut stream, _)) = listener.accept().await {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    accepted_tx.send_modify(|count| *count += 1);
                    let gate = Arc::clone(&gate);
                    let active = Arc::clone(&active);
                    tokio::spawn(async move {
                        read_head(&mut stream).await;
                        gate.acquire().await.unwrap().forget();
                        active.fetch_sub(1, Ordering::SeqCst);
                        stream.write_all(CLOSING_OK).await.unwrap();
                    });
                }
            });
        }
        let client = Arc::new(EgressClient::new().unwrap());
        let sends: Vec<_> = (0..QUEUED_REQUESTS)
            .map(|_| {
                let client = Arc::clone(&client);
                let mut req = request(base.clone(), true);
                req.timeout = Duration::from_secs(60);
                tokio::spawn(async move { client.send(req).await.map(|resp| resp.status) })
            })
            .collect();

        accepted
            .wait_for(|count| *count == MAX_CONCURRENT_REQUESTS)
            .await
            .unwrap();
        gate.add_permits(QUEUED_REQUESTS);
        for send in sends {
            assert_eq!(send.await.unwrap().unwrap(), 200);
        }

        assert_eq!(peak.load(Ordering::SeqCst), MAX_CONCURRENT_REQUESTS);
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
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::FOUND)
                    .insert_header("location", to.as_str()),
            )
            .mount(server)
            .await;
    }

    async fn mount_final(server: &MockServer, at: &str) {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string("final"))
            .mount(server)
            .await;
    }

    async fn user_headers_seen_at(server: &MockServer, at: &str) -> Vec<(String, String)> {
        let requests = server.received_requests().await.unwrap();
        let hit = requests
            .iter()
            .find(|r| r.url.path() == at)
            .expect("final hop must reach the server");
        USER_HEADERS
            .iter()
            .filter_map(|(name, _)| {
                hit.headers
                    .get(*name)
                    .map(|value| ((*name).to_owned(), value.to_str().unwrap().to_owned()))
            })
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
    async fn same_origin_redirect_keeps_user_headers_with_their_values() {
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
            USER_HEADERS
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect::<Vec<_>>()
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
