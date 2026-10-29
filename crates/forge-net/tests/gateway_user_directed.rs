#![allow(clippy::unwrap_used, clippy::panic)]
mod gateway_streaming_support;
mod gateway_support;

use std::sync::Arc;
use std::time::Duration;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    CallClass, FailureObserver, Gateway, GatewayFailure, GatewayProfile, GatewayRequest,
    HostRejection, InvalidRequest, Method, RedirectRefusal, ResolvedTarget, ThrottleHintFamily,
    TransportKind,
};
use gateway_streaming_support::{Step, StubServer, UNLIMITED, sized_head, stream};
use gateway_support::{RecordingObserver, chunked_ok, platform, received, serve_raw_once};
use wiremock::matchers::{any, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BODY_CAP: u64 = 10;
const SHORT_BUDGET: Duration = Duration::from_millis(1);
const BODY_READ_BUDGET: Duration = Duration::from_millis(20);
const HANG_GUARD: Duration = Duration::from_secs(30);

struct Directed {
    gateway: Gateway,
    limiter: Arc<RecordingRateLimiter>,
    observer: Arc<RecordingObserver>,
}

fn directed(allow_local: bool) -> Directed {
    let limiter = Arc::new(RecordingRateLimiter::granting());
    let observer = Arc::new(RecordingObserver::default());
    let gateway = Gateway::without_base(
        limiter.clone(),
        GatewayProfile::user_directed(allow_local),
        Some(observer.clone() as Arc<dyn FailureObserver>),
    )
    .unwrap();
    Directed {
        gateway,
        limiter,
        observer,
    }
}

fn trusted() -> GatewayProfile {
    GatewayProfile::absolute_https(
        CallClass::Media,
        ThrottleHintFamily::RetryAfter,
        Duration::from_secs(60),
    )
}

async fn answering(template: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

async fn redirect(server: &MockServer, from: &str, status: u16, location: &str) {
    Mock::given(path(from))
        .respond_with(ResponseTemplate::new(status).insert_header("Location", location))
        .mount(server)
        .await;
}

async fn land(server: &MockServer, at: &str) {
    Mock::given(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_string("landed"))
        .mount(server)
        .await;
}

fn get(url: String) -> GatewayRequest {
    GatewayRequest::absolute(Method::Get, url)
}

fn header_at(request: &wiremock::Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn redirect_refusal(failure: &GatewayFailure) -> Option<RedirectRefusal> {
    match failure {
        GatewayFailure::Transport(transport) if transport.kind() == TransportKind::Redirect => {
            transport.redirect_refusal()
        }
        other => panic!("expected a redirect transport failure, got {other:?}"),
    }
}

#[tokio::test]
async fn guarded_first_hop_refuses_loopback_mapped_nat64_and_privately_resolving_targets() {
    let server = answering(ResponseTemplate::new(200)).await;
    let port = server.address().port();
    for target in [
        format!("http://127.0.0.1:{port}/x"),
        format!("http://[::ffff:127.0.0.1]:{port}/x"),
        format!("http://[64:ff9b::7f00:1]:{port}/x"),
        format!("http://localhost:{port}/x"),
    ] {
        let failure = directed(false)
            .gateway
            .send(get(target.clone()))
            .await
            .unwrap_err();
        assert_eq!(
            failure,
            GatewayFailure::AddressRefused(HostRejection::PrivateOrSpecial),
            "{target}"
        );
    }
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn local_allowed_first_hop_reaches_a_loopback_literal_and_a_loopback_name() {
    let server = answering(ResponseTemplate::new(200).set_body_string("local")).await;
    let port = server.address().port();
    for target in [
        format!("http://127.0.0.1:{port}/x"),
        format!("http://localhost:{port}/x"),
    ] {
        let response = directed(true).gateway.send(get(target.clone())).await;
        assert_eq!(response.unwrap().body(), b"local", "{target}");
    }
}

#[tokio::test]
async fn local_allowed_redirect_to_a_loopback_name_is_followed() {
    let server = MockServer::start().await;
    let port = server.address().port();
    redirect(
        &server,
        "/start",
        302,
        &format!("http://localhost:{port}/landing"),
    )
    .await;
    land(&server, "/landing").await;

    let response = directed(true)
        .gateway
        .send(get(format!("{}/start", server.uri())))
        .await
        .unwrap();

    assert_eq!(response.body(), b"landed");
}

#[tokio::test]
async fn refused_redirect_targets_report_their_typed_refusal_without_following() {
    for (location, expected) in [
        ("ftp://127.0.0.1/file", RedirectRefusal::UnsupportedScheme),
        (
            "mailto:someone@example.com",
            RedirectRefusal::UnsupportedScheme,
        ),
        (
            "http://user:pw@127.0.0.1/x",
            RedirectRefusal::EmbeddedCredentials,
        ),
        (
            "http://user@127.0.0.1/x",
            RedirectRefusal::EmbeddedCredentials,
        ),
        ("http://[::1", RedirectRefusal::MalformedTarget),
    ] {
        let server = MockServer::start().await;
        redirect(&server, "/start", 302, location).await;

        let failure = directed(true)
            .gateway
            .send(get(format!("{}/start", server.uri())))
            .await
            .unwrap_err();

        assert_eq!(redirect_refusal(&failure), Some(expected), "{location}");
        assert_eq!(received(&server).await.len(), 1, "{location}");
    }
}

#[tokio::test]
async fn a_redirect_loop_is_refused_as_too_many_hops() {
    let server = MockServer::start().await;
    redirect(&server, "/loop", 302, "/loop").await;

    let failure = directed(true)
        .gateway
        .send(get(format!("{}/loop", server.uri())))
        .await
        .unwrap_err();

    assert_eq!(
        redirect_refusal(&failure),
        Some(RedirectRefusal::TooManyHops)
    );
}

#[tokio::test]
async fn non_redirect_three_hundreds_come_back_as_data_without_following_the_location() {
    for status in [300, 304, 305] {
        let server = MockServer::start().await;
        redirect(&server, "/start", status, "/landing").await;
        land(&server, "/landing").await;

        let response = directed(true)
            .gateway
            .send(get(format!("{}/start", server.uri())).any_status_as_data())
            .await
            .unwrap();

        assert_eq!(response.status(), status);
        assert_eq!(received(&server).await.len(), 1, "{status}");
    }
}

#[tokio::test]
async fn any_status_as_data_returns_every_final_status_with_its_body_and_no_feedback() {
    for (template, follow, status, body) in [
        (
            ResponseTemplate::new(404).set_body_string("missing"),
            true,
            404,
            "missing",
        ),
        (
            ResponseTemplate::new(500).set_body_string("boom"),
            true,
            500,
            "boom",
        ),
        (
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "30")
                .set_body_string("slow down"),
            true,
            429,
            "slow down",
        ),
        (
            ResponseTemplate::new(503)
                .insert_header("Retry-After", "30")
                .set_body_string("maintenance"),
            true,
            503,
            "maintenance",
        ),
        (
            ResponseTemplate::new(302)
                .insert_header("Location", "/elsewhere")
                .set_body_string("moved"),
            false,
            302,
            "moved",
        ),
    ] {
        let server = answering(template).await;
        let directed = directed(true);

        let response = directed
            .gateway
            .send(
                get(format!("{}/x", server.uri()))
                    .any_status_as_data()
                    .follow_redirects(follow),
            )
            .await
            .unwrap();

        assert_eq!(
            (response.status(), response.body()),
            (status, body.as_bytes())
        );
        assert!(directed.observer.seen().is_empty(), "{status}");
        assert!(directed.limiter.observed_throttles().is_empty(), "{status}");
    }
}

#[tokio::test]
async fn any_status_as_data_streams_an_error_body_into_the_sink() {
    let server = answering(ResponseTemplate::new(404).set_body_string("missing")).await;
    let directed = directed(true);

    let streamed = stream(
        &directed.gateway,
        get(format!("{}/x", server.uri())).any_status_as_data(),
        UNLIMITED,
    )
    .await;

    assert_eq!(
        (streamed.response().status(), streamed.sink.bytes()),
        (404, b"missing".to_vec())
    );
}

#[tokio::test]
async fn truncate_body_at_cap_keeps_the_first_cap_bytes_and_flags_only_real_overflow() {
    for (length, truncated) in [(9, false), (10, false), (11, true), (4096, true)] {
        let body = "x".repeat(length);
        let server = answering(ResponseTemplate::new(200).set_body_string(body)).await;

        let response = directed(true)
            .gateway
            .send(
                get(format!("{}/x", server.uri()))
                    .body_cap(BODY_CAP)
                    .truncate_body_at_cap(),
            )
            .await
            .unwrap();

        assert_eq!(response.truncated(), truncated, "{length}");
        assert_eq!(
            response.body().len(),
            length.min(usize::try_from(BODY_CAP).unwrap()),
            "{length}"
        );
    }
}

#[tokio::test]
async fn truncate_body_at_cap_flags_a_chunked_body_that_passes_the_cap() {
    let base = serve_raw_once(chunked_ok(&[b"123456".to_vec(), b"7890AB".to_vec()])).await;

    let response = directed(true)
        .gateway
        .send(
            get(format!("{base}/x"))
                .body_cap(BODY_CAP)
                .truncate_body_at_cap(),
        )
        .await
        .unwrap();

    assert_eq!(
        (response.body(), response.truncated()),
        (&b"1234567890"[..], true)
    );
}

#[tokio::test]
async fn total_budget_times_out_a_response_that_never_arrives() {
    let server = StubServer::serving(vec![Step::Hold]).await;

    let failure = tokio::time::timeout(
        HANG_GUARD,
        directed(true)
            .gateway
            .send(get(format!("{}/x", server.uri())).total_budget(SHORT_BUDGET)),
    )
    .await
    .expect("the total budget never fired")
    .unwrap_err();

    assert!(
        matches!(&failure, GatewayFailure::Transport(t) if t.kind() == TransportKind::Timeout),
        "{failure:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn total_budget_longer_than_the_class_budget_outlasts_it() {
    let budget = Duration::from_secs(30);
    let server = StubServer::serving(vec![Step::Hold]).await;
    let started = tokio::time::Instant::now();

    let failure = directed(true)
        .gateway
        .send(get(format!("{}/x", server.uri())).total_budget(budget))
        .await
        .unwrap_err();

    assert!(
        matches!(&failure, GatewayFailure::Transport(t) if t.kind() == TransportKind::Timeout),
        "{failure:?}"
    );
    assert_eq!(started.elapsed(), budget);
}

#[tokio::test]
async fn total_budget_also_bounds_the_body_read() {
    let server = StubServer::serving(vec![
        sized_head(10),
        Step::Send(b"half".to_vec()),
        Step::Hold,
    ])
    .await;

    let failure = directed(true)
        .gateway
        .send(
            get(format!("{}/x", server.uri()))
                .total_budget(BODY_READ_BUDGET)
                .truncate_body_at_cap(),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(&failure, GatewayFailure::Transport(t) if t.kind() == TransportKind::Timeout),
        "{failure:?}"
    );
}

#[tokio::test]
async fn user_headers_reach_the_server_including_credential_names_in_any_case() {
    let server = answering(ResponseTemplate::new(200)).await;

    directed(true)
        .gateway
        .send(
            get(format!("{}/x", server.uri()))
                .user_header("AUTHORIZATION", "Bearer user-token")
                .user_header("Cookie", "sid=user-cookie")
                .user_header("x-custom", "plain-value"),
        )
        .await
        .unwrap();

    let seen = &received(&server).await[0];
    assert_eq!(
        [
            header_at(seen, "authorization"),
            header_at(seen, "cookie"),
            header_at(seen, "x-custom"),
        ],
        [
            Some("Bearer user-token".to_owned()),
            Some("sid=user-cookie".to_owned()),
            Some("plain-value".to_owned()),
        ]
    );
}

#[tokio::test]
async fn credential_user_headers_are_masked_whole_and_as_the_bare_token_in_excerpts() {
    let echo = "whole=Bearer TOKEN-1 bare=TOKEN-1 cookie=sid=COOKIE-1 proxy=PROXY-1 plain=PLAIN-1";
    let server = answering(ResponseTemplate::new(400).set_body_string(echo)).await;

    let failure = directed(true)
        .gateway
        .send(
            get(format!("{}/x", server.uri()))
                .user_header("Authorization", "Bearer TOKEN-1")
                .user_header("COOKIE", "sid=COOKIE-1")
                .user_header("proxy-authorization", "Basic PROXY-1")
                .user_header("x-plain", "PLAIN-1"),
        )
        .await
        .unwrap_err();

    assert_eq!(
        failure.excerpt().unwrap().as_str(),
        "whole=*** bare=*** cookie=*** proxy=*** plain=PLAIN-1"
    );
}

#[test]
fn request_debug_never_prints_a_user_header_value() {
    let request = get("https://example.com/x".to_owned())
        .user_header("Authorization", "Bearer DEBUG-SECRET")
        .user_header("x-plain", "DEBUG-PLAIN");

    let rendered = format!("{request:?}");

    assert!(!rendered.contains("DEBUG-SECRET"), "{rendered}");
    assert!(!rendered.contains("DEBUG-PLAIN"), "{rendered}");
}

#[tokio::test]
async fn user_headers_follow_a_same_origin_hop_and_are_stripped_from_a_cross_origin_one() {
    for cross_origin in [false, true] {
        let origin = MockServer::start().await;
        let other = MockServer::start().await;
        let landing = if cross_origin { &other } else { &origin };
        redirect(
            &origin,
            "/start",
            302,
            &format!("{}/landing", landing.uri()),
        )
        .await;
        land(landing, "/landing").await;

        directed(true)
            .gateway
            .send(
                get(format!("{}/start", origin.uri()))
                    .user_header("Authorization", "Bearer hop-token")
                    .user_header("cookie", "sid=hop-cookie")
                    .user_header("x-custom", "hop-plain"),
            )
            .await
            .unwrap();

        let landed = received(landing)
            .await
            .into_iter()
            .find(|request| request.url.path() == "/landing")
            .unwrap();
        for name in ["authorization", "cookie", "x-custom"] {
            assert_eq!(
                header_at(&landed, name).is_some(),
                !cross_origin,
                "{name} cross_origin={cross_origin}"
            );
        }
    }
}

#[tokio::test]
async fn user_headers_are_refused_off_the_user_directed_profile_before_the_limiter() {
    let server = answering(ResponseTemplate::new(200)).await;
    for profile in [platform(), trusted()] {
        for streaming in [false, true] {
            let limiter = Arc::new(RecordingRateLimiter::granting());
            let gateway = Gateway::new(&server.uri(), limiter.clone(), profile, None).unwrap();
            let request = GatewayRequest::get("x").user_header("x-custom", "value");

            let failure = if streaming {
                stream(&gateway, request, UNLIMITED).await.failure()
            } else {
                gateway.send(request).await.unwrap_err()
            };

            assert_eq!(
                failure,
                GatewayFailure::InvalidRequest(InvalidRequest::UserHeaderNotAllowed),
                "{profile:?} streaming={streaming}"
            );
            assert_eq!(limiter.acquire_count(), 0);
        }
    }
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn plain_credential_headers_are_refused_on_every_profile() {
    let server = answering(ResponseTemplate::new(200)).await;
    for profile in [platform(), trusted(), GatewayProfile::user_directed(true)] {
        for name in ["Authorization", "proxy-authorization", "COOKIE"] {
            let gateway = Gateway::new(
                &server.uri(),
                Arc::new(RecordingRateLimiter::granting()),
                profile,
                None,
            )
            .unwrap();

            let failure = gateway
                .send(GatewayRequest::get("x").header(name, "secret"))
                .await
                .unwrap_err();

            assert_eq!(
                failure,
                GatewayFailure::InvalidRequest(InvalidRequest::SensitiveHeader),
                "{profile:?} {name}"
            );
        }
    }
    assert!(received(&server).await.is_empty());
}

#[test]
fn resolved_target_absolute_parses_scheme_host_and_path_and_refuses_userinfo() {
    for (raw, expected) in [
        (
            "https://Example.COM:8443/a/b?q=1",
            Ok(("https", "example.com", "/a/b")),
        ),
        ("http://[::1]:8080/", Ok(("http", "[::1]", "/"))),
        ("ftp://files.example/x", Ok(("ftp", "files.example", "/x"))),
        (
            "http://user@example.com/",
            Err(InvalidRequest::EmbeddedCredentials),
        ),
        (
            "https://user:pw@example.com/",
            Err(InvalidRequest::EmbeddedCredentials),
        ),
        ("not a url", Err(InvalidRequest::MalformedUrl)),
        ("", Err(InvalidRequest::MalformedUrl)),
    ] {
        let parsed = ResolvedTarget::absolute(raw);
        let parsed = parsed
            .as_ref()
            .map(|target| (target.scheme(), target.host(), target.path()))
            .map_err(Clone::clone);
        assert_eq!(parsed, expected, "{raw}");
    }
}
