#![allow(clippy::panic)]
mod gateway_streaming_support;
mod gateway_support;

use std::time::Duration;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    CallClass, ExpectedRefusal, FailureContext, GatewayFailure, GatewayRequest, InvalidRequest,
    Method, RateLimitOutcome, RequestBody, Secret, StreamFailure, TransportKind,
};
use gateway_streaming_support::{MemorySink, SinkRefused, UNLIMITED, stream, transport_kind};
use gateway_support::{Harness, platform, received};
use serde_json::json;
use wiremock::matchers::{any, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MAX_REDIRECT_HOPS: usize = 10;
const MASK: &str = "***";

async fn serving(template: ResponseTemplate) -> MockServer {
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

async fn request_to(server: &MockServer, at: &str) -> wiremock::Request {
    received(server)
        .await
        .into_iter()
        .find(|request| request.url.path() == at)
        .unwrap_or_else(|| panic!("no request reached {at}"))
}

fn header<'a>(request: &'a wiremock::Request, name: &str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
}

#[tokio::test]
async fn invalid_targets_and_headers_are_refused_before_the_limiter_or_the_network() {
    let server = serving(ResponseTemplate::new(200)).await;
    let uri = server.uri();
    for (request, expected) in [
        (
            GatewayRequest::get("asset?v=1"),
            InvalidRequest::QueryInPath,
        ),
        (
            GatewayRequest::absolute(Method::Get, format!("{uri}/asset")),
            InvalidRequest::AbsoluteUrlNotAllowed,
        ),
        (
            GatewayRequest::get("asset").header("bad name", "v"),
            InvalidRequest::InvalidHeader,
        ),
        (
            GatewayRequest::get("asset").bearer(Secret::new("tok\nen")),
            InvalidRequest::InvalidHeader,
        ),
    ] {
        let harness = Harness::new(&uri, platform());
        let streamed = stream(&harness.gateway, request, UNLIMITED).await;
        assert_eq!(streamed.failure(), GatewayFailure::InvalidRequest(expected));
        assert_eq!(harness.limiter.acquire_count(), 0, "{expected:?}");
    }
    assert!(received(&server).await.is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_refusing_limiter_fails_the_stream_with_budget_exhausted_and_sends_nothing() {
    let server = serving(ResponseTemplate::new(200)).await;
    let wait = Duration::from_millis(42_137);
    for (limiter, request, expected) in [
        (
            RecordingRateLimiter::exhausted(),
            GatewayRequest::get("asset"),
            None,
        ),
        (
            RecordingRateLimiter::failing(),
            GatewayRequest::get("asset"),
            None,
        ),
        (
            RecordingRateLimiter::answering(Ok(RateLimitOutcome::Throttled { wait_for: wait })),
            GatewayRequest::get("asset").without_limiter_wait(),
            Some(wait),
        ),
    ] {
        let harness = Harness::with_limiter(&server.uri(), platform(), limiter);
        let started = tokio::time::Instant::now();
        let streamed = stream(&harness.gateway, request, UNLIMITED).await;
        assert_eq!(
            streamed.failure(),
            GatewayFailure::BudgetExhausted {
                retry_after: expected
            }
        );
        assert_eq!(started.elapsed(), Duration::ZERO);
    }
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn see_other_family_redirects_are_followed_as_a_bodyless_get_into_the_sink() {
    for status in [301, 302, 303] {
        let server = MockServer::start().await;
        redirect(&server, "/start", status, "/landing").await;
        land(&server, "/landing").await;
        let harness = Harness::new(&server.uri(), platform());
        let streamed = stream(
            &harness.gateway,
            GatewayRequest::post("start").body(RequestBody::json(&json!({"a": 1})).unwrap()),
            UNLIMITED,
        )
        .await;
        assert_eq!(streamed.sink.bytes(), b"landed", "status {status}");
        let landing = request_to(&server, "/landing").await;
        assert_eq!(
            (landing.method.as_str(), landing.body.is_empty()),
            ("GET", true),
            "status {status}"
        );
    }
}

#[tokio::test]
async fn ten_redirect_hops_are_streamed_and_the_eleventh_is_refused() {
    for (hops, refused) in [(MAX_REDIRECT_HOPS, false), (MAX_REDIRECT_HOPS + 1, true)] {
        let server = MockServer::start().await;
        for hop in 0..hops {
            redirect(&server, &format!("/h{hop}"), 302, &format!("/h{}", hop + 1)).await;
        }
        land(&server, &format!("/h{hops}")).await;
        let harness = Harness::new(&server.uri(), platform());
        let streamed = stream(&harness.gateway, GatewayRequest::get("h0"), UNLIMITED).await;
        if refused {
            assert_eq!(transport_kind(&streamed.failure()), TransportKind::Redirect);
            assert!(streamed.sink.bytes().is_empty());
        } else {
            assert_eq!(streamed.sink.bytes(), b"landed");
        }
    }
}

#[tokio::test]
async fn a_disallowed_redirect_target_is_refused_without_following() {
    for location in ["http://192.0.2.1/plain-http", "ftp://192.0.2.1/file"] {
        let server = MockServer::start().await;
        redirect(&server, "/start", 302, location).await;
        let harness = Harness::new(&server.uri(), platform());
        let streamed = stream(&harness.gateway, GatewayRequest::get("start"), UNLIMITED).await;
        assert_eq!(
            transport_kind(&streamed.failure()),
            TransportKind::Redirect,
            "{location}"
        );
        assert_eq!(received(&server).await.len(), 1, "{location}");
    }
}

#[tokio::test]
async fn a_streamed_redirect_keeps_headers_on_the_same_origin_and_strips_them_across_origins() {
    let origin = MockServer::start().await;
    let other = MockServer::start().await;
    redirect(&origin, "/start", 302, "/same").await;
    redirect(&origin, "/same", 302, &format!("{}/away", other.uri())).await;
    land(&other, "/away").await;
    let harness = Harness::new(&origin.uri(), platform());

    let streamed = stream(
        &harness.gateway,
        GatewayRequest::get("start")
            .header("x-trace", "trace-1")
            .bearer(Secret::new("asset-token")),
        UNLIMITED,
    )
    .await;

    assert_eq!(streamed.sink.bytes(), b"landed");
    let same = request_to(&origin, "/same").await;
    assert_eq!(
        (header(&same, "x-trace"), header(&same, "authorization")),
        (Some("trace-1"), Some("Bearer asset-token"))
    );
    let away = request_to(&other, "/away").await;
    assert_eq!(
        (header(&away, "x-trace"), header(&away, "authorization")),
        (None, None)
    );
}

#[tokio::test]
async fn an_unfollowed_redirect_is_an_unexpected_status_and_the_sink_stays_empty() {
    let server = MockServer::start().await;
    redirect(&server, "/start", 302, "/landing").await;
    land(&server, "/landing").await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream(
        &harness.gateway,
        GatewayRequest::get("start").follow_redirects(false),
        UNLIMITED,
    )
    .await;

    assert!(
        matches!(
            streamed.failure(),
            GatewayFailure::UnexpectedStatus { status: 302, .. }
        ),
        "{:?}",
        streamed.failure()
    );
    assert!(streamed.sink.bytes().is_empty());
    assert_eq!(received(&server).await.len(), 1);
}

#[tokio::test]
async fn non_success_statuses_classify_exactly_as_the_buffered_send_does() {
    for template in [
        ResponseTemplate::new(401).set_body_string("who are you"),
        ResponseTemplate::new(403).set_body_string("rate limit exceeded"),
        ResponseTemplate::new(404).set_body_string("no such asset"),
        ResponseTemplate::new(422).set_body_string("bad input"),
        ResponseTemplate::new(500).set_body_string("server exploded"),
        ResponseTemplate::new(304),
        ResponseTemplate::new(429)
            .insert_header("Retry-After", "12")
            .set_body_string("slow down"),
        ResponseTemplate::new(429).set_body_string("slow down"),
        ResponseTemplate::new(503)
            .insert_header("Retry-After", "7")
            .set_body_string("maintenance"),
        ResponseTemplate::new(503).set_body_string("maintenance"),
    ] {
        let server = serving(template).await;
        let buffered = Harness::new(&server.uri(), platform());
        let streaming = Harness::new(&server.uri(), platform());

        let expected = buffered.fail(GatewayRequest::get("asset")).await;
        let streamed = stream(&streaming.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

        assert_eq!(streamed.failure(), expected);
        assert_eq!(
            streaming.limiter.observed_throttles(),
            buffered.limiter.observed_throttles(),
            "{expected:?}"
        );
        assert!(streamed.sink.bytes().is_empty(), "{expected:?}");
    }
}

#[tokio::test]
async fn a_rate_limited_stream_feeds_the_hinted_wait_to_the_limiter() {
    let server = serving(ResponseTemplate::new(429).insert_header("Retry-After", "12")).await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

    assert!(matches!(
        streamed.failure(),
        GatewayFailure::RateLimited { status: 429, .. }
    ));
    assert_eq!(
        harness.limiter.observed_throttles(),
        vec![Duration::from_secs(12)]
    );
}

#[tokio::test]
async fn an_error_excerpt_masks_the_credential_and_every_masked_secret() {
    let server = serving(
        ResponseTemplate::new(401).set_body_string("token asset-token rejected; key signed-key"),
    )
    .await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream(
        &harness.gateway,
        GatewayRequest::get("asset")
            .bearer(Secret::new("asset-token"))
            .mask(Secret::new("signed-key")),
        UNLIMITED,
    )
    .await;

    let failure = streamed.failure();
    assert_eq!(
        failure.excerpt().unwrap().as_str(),
        format!("token {MASK} rejected; key {MASK}")
    );
}

#[tokio::test]
async fn an_error_excerpt_is_read_under_its_own_cap_not_the_caller_cap() {
    let server = serving(ResponseTemplate::new(500).set_body_string("server exploded")).await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), 1).await;

    let failure = streamed.failure();
    assert!(
        matches!(failure, GatewayFailure::ServerError { status: 500, .. }),
        "{failure:?}"
    );
    assert_eq!(failure.excerpt().unwrap().as_str(), "server exploded");
}

#[tokio::test]
async fn every_gateway_failure_reaches_the_observer_with_the_request_context() {
    let not_found = serving(ResponseTemplate::new(404)).await;
    let oversized = serving(ResponseTemplate::new(200).set_body_string("0123456789")).await;
    for (uri, limiter, cap, expected) in [
        (
            not_found.uri(),
            RecordingRateLimiter::granting(),
            UNLIMITED,
            GatewayFailure::NotFound {
                excerpt: Default::default(),
            },
        ),
        (
            oversized.uri(),
            RecordingRateLimiter::granting(),
            9,
            GatewayFailure::ResponseTooLarge { cap: 9 },
        ),
        (
            not_found.uri(),
            RecordingRateLimiter::exhausted(),
            UNLIMITED,
            GatewayFailure::BudgetExhausted { retry_after: None },
        ),
    ] {
        let harness = Harness::with_limiter(&uri, platform(), limiter);
        let request = GatewayRequest::get("asset")
            .class(CallClass::Media)
            .label("asset download");

        let streamed = stream(&harness.gateway, request, cap).await;

        assert_eq!(streamed.failure(), expected);
        assert_eq!(
            harness.observer.seen(),
            vec![(
                expected,
                FailureContext {
                    method: Method::Get,
                    class: CallClass::Media,
                    label: Some("asset download"),
                }
            )]
        );
    }
}

#[tokio::test]
async fn the_observer_context_falls_back_to_the_profile_default_class() {
    let server = serving(ResponseTemplate::new(404)).await;
    let harness = Harness::new(&server.uri(), platform());

    stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

    let seen = harness.observer.seen();
    assert_eq!(
        seen.iter()
            .map(|(_, context)| context.class)
            .collect::<Vec<_>>(),
        vec![CallClass::Interactive]
    );
}

#[tokio::test]
async fn only_a_matching_expected_refusal_keeps_a_stream_failure_from_the_observer() {
    let server = serving(ResponseTemplate::new(404).set_body_string("no such release")).await;
    for (refusal, observed) in [
        (ExpectedRefusal::status(404), 0),
        (ExpectedRefusal::status_with_excerpt(404, "RELEASE"), 0),
        (ExpectedRefusal::status_with_excerpt(404, "asset"), 1),
        (ExpectedRefusal::status(500), 1),
    ] {
        let harness = Harness::new(&server.uri(), platform());
        let streamed = stream(
            &harness.gateway,
            GatewayRequest::get("asset").expect_refusal(refusal.clone()),
            UNLIMITED,
        )
        .await;
        assert!(
            matches!(streamed.failure(), GatewayFailure::NotFound { .. }),
            "{refusal:?}"
        );
        assert_eq!(harness.observer.seen().len(), observed, "{refusal:?}");
    }
}

#[tokio::test]
async fn a_sink_rejection_is_returned_as_a_sink_failure_and_never_observed() {
    let server = serving(ResponseTemplate::new(200).set_body_string("payload")).await;
    let harness = Harness::new(&server.uri(), platform());
    let mut sink = MemorySink::refusing_chunk(0);

    let outcome = harness
        .gateway
        .send_streaming(
            GatewayRequest::get("asset"),
            UNLIMITED,
            &mut sink,
            |_| {},
            &tokio_util::sync::CancellationToken::new(),
        )
        .await;

    assert!(
        matches!(outcome, Err(StreamFailure::Sink(SinkRefused { chunk: 0 }))),
        "{outcome:?}"
    );
    assert!(harness.observer.seen().is_empty());
}
