#![allow(clippy::panic)]
mod gateway_support;

use std::time::Duration;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    CallClass, FailureContext, GatewayFailure, GatewayProfile, GatewayRequest, GatewayTimings,
    Method, RateLimitOutcome, ThrottleHintFamily, TransportKind,
};
use gateway_support::{
    Harness, accept_request_head, platform, received, refused_base, silent_listener,
};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const NEVER: Duration = Duration::from_secs(3600);
const SHORT_TOTAL_BUDGET: Duration = Duration::from_millis(200);
const HANG_GUARD: Duration = Duration::from_secs(30);

fn catalog_default() -> GatewayProfile {
    GatewayProfile::platform_api(
        CallClass::Catalog,
        ThrottleHintFamily::RetryAfter,
        Duration::from_secs(5),
    )
}

fn transport_kind(failure: &GatewayFailure) -> TransportKind {
    match failure {
        GatewayFailure::Transport(transport) => transport.kind(),
        other => panic!("expected a transport failure, got {other:?}"),
    }
}

async fn serving(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn unanswered_request_fails_with_a_transport_timeout_once_the_budget_elapses() {
    let (_listener, base) = silent_listener();
    let profile = platform().with_timings(GatewayTimings {
        interactive_total: SHORT_TOTAL_BUDGET,
        ..GatewayTimings::DEFAULT
    });
    let harness = Harness::new(&base, profile);
    let started = tokio::time::Instant::now();

    let failure = tokio::time::timeout(HANG_GUARD, harness.fail(GatewayRequest::get("slow")))
        .await
        .expect("the total budget never fired");

    assert_eq!(
        (
            transport_kind(&failure),
            started.elapsed() >= SHORT_TOTAL_BUDGET
        ),
        (TransportKind::Timeout, true)
    );
}

#[tokio::test]
async fn refused_connection_is_a_transport_connect_failure() {
    let harness = Harness::new(&refused_base(), platform());
    let failure = harness.fail(GatewayRequest::get("x")).await;
    assert_eq!(transport_kind(&failure), TransportKind::Connect);
}

#[tokio::test]
async fn pre_cancelled_request_never_reaches_the_limiter_or_the_observer() {
    let server = serving(200).await;
    let harness = Harness::new(&server.uri(), platform());
    let token = CancellationToken::new();
    token.cancel();
    let failure = harness
        .fail(GatewayRequest::get("x").cancel_on(token))
        .await;
    assert_eq!(failure, GatewayFailure::Cancelled);
    assert_eq!(harness.limiter.acquire_count(), 0);
    assert!(harness.observer.seen().is_empty());
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn cancelling_while_the_response_is_pending_yields_cancelled_without_observer() {
    let (listener, base) = silent_listener();
    let harness = Harness::new(&base, platform());
    let token = CancellationToken::new();
    let gateway = harness.gateway.clone();
    let request = GatewayRequest::get("pending").cancel_on(token.clone());
    let send = tokio::spawn(async move { gateway.send(request).await });
    let connection = tokio::task::spawn_blocking(move || accept_request_head(&listener))
        .await
        .unwrap();
    token.cancel();
    let outcome = send.await.unwrap();
    assert_eq!(outcome.unwrap_err(), GatewayFailure::Cancelled);
    assert_eq!(harness.limiter.acquire_count(), 1);
    assert!(harness.observer.seen().is_empty());
    drop(connection);
}

#[tokio::test]
async fn cancelling_during_a_limiter_wait_yields_cancelled_and_sends_nothing() {
    let server = serving(200).await;
    let limiter =
        RecordingRateLimiter::answering(Ok(RateLimitOutcome::Throttled { wait_for: NEVER }));
    let harness = Harness::with_limiter(&server.uri(), platform(), limiter);
    let token = CancellationToken::new();
    let gateway = harness.gateway.clone();
    let request = GatewayRequest::get("x").cancel_on(token.clone());
    let send = tokio::spawn(async move { gateway.send(request).await });
    while harness.limiter.acquire_count() == 0 {
        tokio::task::yield_now().await;
    }
    token.cancel();
    assert_eq!(send.await.unwrap().unwrap_err(), GatewayFailure::Cancelled);
    assert!(harness.observer.seen().is_empty());
    assert!(received(&server).await.is_empty());
}

#[tokio::test]
async fn observer_is_silent_on_success() {
    let server = serving(204).await;
    let harness = Harness::new(&server.uri(), platform());
    harness.ok(GatewayRequest::get("x").label("probe")).await;
    assert!(harness.observer.seen().is_empty());
}

#[tokio::test]
async fn observer_sees_each_failure_exactly_once_with_its_request_context() {
    let not_found = serving(404).await;
    let throttled = serving(429).await;
    let refused = refused_base();
    for (base, limiter, request, context) in [
        (
            not_found.uri(),
            RecordingRateLimiter::granting(),
            GatewayRequest::post("x")
                .class(CallClass::Media)
                .label("tts.synthesize"),
            FailureContext {
                method: Method::Post,
                class: CallClass::Media,
                label: Some("tts.synthesize"),
            },
        ),
        (
            throttled.uri(),
            RecordingRateLimiter::granting(),
            GatewayRequest::get("x"),
            FailureContext {
                method: Method::Get,
                class: CallClass::Catalog,
                label: None,
            },
        ),
        (
            not_found.uri(),
            RecordingRateLimiter::granting(),
            GatewayRequest::patch("x?bad").label("patch.bad"),
            FailureContext {
                method: Method::Patch,
                class: CallClass::Catalog,
                label: Some("patch.bad"),
            },
        ),
        (
            not_found.uri(),
            RecordingRateLimiter::exhausted(),
            GatewayRequest::delete("x").class(CallClass::OAuth),
            FailureContext {
                method: Method::Delete,
                class: CallClass::OAuth,
                label: None,
            },
        ),
        (
            refused.clone(),
            RecordingRateLimiter::granting(),
            GatewayRequest::put("x").label("upload"),
            FailureContext {
                method: Method::Put,
                class: CallClass::Catalog,
                label: Some("upload"),
            },
        ),
    ] {
        let harness = Harness::with_limiter(&base, catalog_default(), limiter);
        let failure = harness.fail(request).await;
        assert_eq!(harness.observer.seen(), vec![(failure, context)]);
    }
}
