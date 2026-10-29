mod gateway_streaming_support;
mod gateway_support;

use std::time::Duration;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    BodyProgress, CallClass, GatewayFailure, GatewayProfile, GatewayRequest, GatewayTimings,
    HeaderLookup, RateLimitOutcome, TransportKind,
};
use gateway_streaming_support::{
    Gate, MemorySink, Pace, Step, StubServer, UNLIMITED, chunk_frame, chunked_end, chunked_head,
    sized_body, sized_head, spawn_stream, spin_until, stream, stream_into, stream_with,
    transport_kind,
};
use gateway_support::{Harness, platform};
use tokio_util::sync::CancellationToken;

const NEVER: Duration = Duration::from_secs(3600);
const SHORT_IDLE: Duration = Duration::from_millis(200);
const HANG_GUARD: Duration = Duration::from_secs(30);
const TOTAL_BUDGET: Duration = Duration::from_millis(1);
const PAST_TOTAL_BUDGET: Duration = Duration::from_millis(20);

fn idle_bounded() -> GatewayProfile {
    platform().with_timings(GatewayTimings {
        streaming_read_idle: SHORT_IDLE,
        ..GatewayTimings::DEFAULT
    })
}

fn progress(received: u64, declared_length: Option<u64>) -> BodyProgress {
    BodyProgress {
        received,
        declared_length,
    }
}

#[tokio::test]
async fn a_success_body_streams_into_the_sink_with_status_headers_and_byte_count() {
    let pace = Pace::default();
    let server = StubServer::serving(pace.chunked_body(&[b"abc", b"defg", b"hi"])).await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream_into(
        &harness.gateway,
        GatewayRequest::get("asset"),
        UNLIMITED,
        pace.sink(),
    )
    .await;

    let response = streamed.response();
    assert_eq!(
        (
            response.status(),
            response.header_value("X-Asset"),
            response.received(),
            streamed.sink.bytes(),
        ),
        (200, Some("build-7"), 9, b"abcdefghi".to_vec())
    );
}

#[tokio::test]
async fn the_cap_refuses_a_declared_or_cumulative_overrun_without_handing_the_sink_bytes_past_it() {
    let ten: &[&[u8]] = &[b"01234", b"56789"];
    let twelve: &[&[u8]] = &[b"aaaa", b"bbbb", b"cccc"];
    for (chunked, pieces, cap, delivered, accepted) in [
        (false, ten, 9, b"".as_slice(), false),
        (false, ten, 10, b"0123456789".as_slice(), true),
        (false, &[][..], 0, b"".as_slice(), true),
        (true, twelve, 8, b"aaaabbbb".as_slice(), false),
        (true, twelve, 11, b"aaaabbbb".as_slice(), false),
        (true, twelve, 12, b"aaaabbbbcccc".as_slice(), true),
    ] {
        let pace = Pace::default();
        let steps = if chunked {
            pace.chunked_body(pieces)
        } else {
            pace.sized_body(pieces)
        };
        let server = StubServer::serving(steps).await;
        let harness = Harness::new(&server.uri(), platform());

        let streamed = stream_into(
            &harness.gateway,
            GatewayRequest::get("asset"),
            cap,
            pace.sink(),
        )
        .await;

        assert_eq!(streamed.sink.bytes(), delivered, "cap {cap}");
        if accepted {
            assert_eq!(
                streamed.response().received(),
                streamed.sink.len(),
                "cap {cap}"
            );
        } else {
            assert_eq!(
                streamed.failure(),
                GatewayFailure::ResponseTooLarge { cap },
                "cap {cap}"
            );
        }
    }
}

#[tokio::test]
async fn a_declared_overrun_is_refused_before_any_progress_is_reported() {
    let server = StubServer::serving(sized_body(&[b"0123456789"])).await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), 9).await;

    assert!(streamed.progress.is_empty(), "{:?}", streamed.progress);
}

#[tokio::test]
async fn progress_fires_once_at_zero_and_then_after_every_delivered_chunk() {
    for (chunked, pieces, expected) in [
        (
            false,
            &[b"ab".as_slice(), b"cdef"][..],
            vec![
                progress(0, Some(6)),
                progress(2, Some(6)),
                progress(6, Some(6)),
            ],
        ),
        (
            true,
            &[b"abc".as_slice(), b"d"][..],
            vec![progress(0, None), progress(3, None), progress(4, None)],
        ),
        (false, &[][..], vec![progress(0, Some(0))]),
    ] {
        let pace = Pace::default();
        let steps = if chunked {
            pace.chunked_body(pieces)
        } else {
            pace.sized_body(pieces)
        };
        let server = StubServer::serving(steps).await;
        let harness = Harness::new(&server.uri(), platform());

        let streamed = stream_into(
            &harness.gateway,
            GatewayRequest::get("asset"),
            UNLIMITED,
            pace.sink(),
        )
        .await;

        assert_eq!(streamed.progress, expected);
    }
}

#[tokio::test]
async fn response_headers_get_the_read_idle_budget_and_no_more() {
    let server = StubServer::serving(vec![Step::Hold]).await;
    let harness = Harness::new(&server.uri(), idle_bounded());
    let started = tokio::time::Instant::now();

    let streamed = tokio::time::timeout(
        HANG_GUARD,
        stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED),
    )
    .await
    .expect("the read-idle budget never fired");

    assert_eq!(
        (
            transport_kind(&streamed.failure()),
            started.elapsed() >= SHORT_IDLE
        ),
        (TransportKind::Timeout, true)
    );
}

#[tokio::test]
async fn headers_that_never_arrive_time_out_and_reach_the_observer() {
    let server = StubServer::serving(vec![Step::Hold]).await;
    let harness = Harness::new(&server.uri(), idle_bounded());

    let streamed = tokio::time::timeout(
        HANG_GUARD,
        stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED),
    )
    .await
    .expect("the read-idle budget never fired");

    let failure = streamed.failure();
    assert_eq!(transport_kind(&failure), TransportKind::Timeout);
    assert_eq!(
        harness
            .observer
            .seen()
            .into_iter()
            .map(|(seen, _)| seen)
            .collect::<Vec<_>>(),
        vec![failure]
    );
}

#[tokio::test]
async fn a_gap_between_body_chunks_gets_the_read_idle_budget_and_no_more() {
    let server =
        StubServer::serving(vec![chunked_head(200), chunk_frame(b"first"), Step::Hold]).await;
    let harness = Harness::new(&server.uri(), idle_bounded());

    let streamed = tokio::time::timeout(
        HANG_GUARD,
        stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED),
    )
    .await
    .expect("the read-idle budget never fired");
    let failed_at = tokio::time::Instant::now();

    let last_chunk_at = streamed.sink.last_accepted_at().unwrap();
    assert_eq!(
        (
            transport_kind(&streamed.failure()),
            streamed.sink.bytes(),
            failed_at - last_chunk_at >= SHORT_IDLE
        ),
        (TransportKind::Timeout, b"first".to_vec(), true)
    );
}

#[tokio::test]
async fn a_body_still_flowing_after_the_request_total_budget_completes() {
    let gate = Gate::default();
    let server = StubServer::serving(vec![
        chunked_head(200),
        chunk_frame(b"part-1"),
        gate.step(),
        chunk_frame(b"part-2"),
        chunked_end(),
    ])
    .await;
    let harness = Harness::new(&server.uri(), platform());
    let sink = MemorySink::default();
    let task = spawn_stream(
        &harness.gateway,
        GatewayRequest::get("asset")
            .class(CallClass::Media)
            .total_budget(TOTAL_BUDGET),
        sink.clone(),
        CancellationToken::new(),
    );
    spin_until(|| sink.len() > 0, "first chunk never delivered").await;
    tokio::time::sleep(PAST_TOTAL_BUDGET).await;

    gate.open();
    let streamed = task.await.unwrap();

    assert_eq!(streamed.response().received(), 12);
}

#[tokio::test]
async fn an_already_cancelled_stream_never_reaches_the_limiter_the_network_or_the_observer() {
    let server = StubServer::serving(sized_body(&[b"payload"])).await;
    for request_level in [false, true] {
        let harness = Harness::new(&server.uri(), platform());
        let token = CancellationToken::new();
        token.cancel();
        let (request, cancel) = if request_level {
            (
                GatewayRequest::get("asset").cancel_on(token),
                CancellationToken::new(),
            )
        } else {
            (GatewayRequest::get("asset"), token)
        };

        let streamed = stream_with(
            &harness.gateway,
            request,
            UNLIMITED,
            MemorySink::default(),
            &cancel,
        )
        .await;

        assert_eq!(streamed.failure(), GatewayFailure::Cancelled);
        assert_eq!(harness.limiter.acquire_count(), 0);
        assert!(harness.observer.seen().is_empty());
    }
    assert!(server.heads().is_empty());
}

fn cancellable(request_level: bool) -> (GatewayRequest, CancellationToken, CancellationToken) {
    let trigger = CancellationToken::new();
    if request_level {
        (
            GatewayRequest::get("asset").cancel_on(trigger.clone()),
            CancellationToken::new(),
            trigger,
        )
    } else {
        (GatewayRequest::get("asset"), trigger.clone(), trigger)
    }
}

#[tokio::test]
async fn cancelling_during_the_limiter_wait_yields_cancelled_and_sends_nothing() {
    let server = StubServer::serving(sized_body(&[b"payload"])).await;
    for request_level in [false, true] {
        let limiter =
            RecordingRateLimiter::answering(Ok(RateLimitOutcome::Throttled { wait_for: NEVER }));
        let harness = Harness::with_limiter(&server.uri(), platform(), limiter);
        let (request, cancel, trigger) = cancellable(request_level);
        let task = spawn_stream(&harness.gateway, request, MemorySink::default(), cancel);
        spin_until(
            || harness.limiter.acquire_count() > 0,
            "limiter never asked",
        )
        .await;

        trigger.cancel();
        let streamed = task.await.unwrap();

        assert_eq!(streamed.failure(), GatewayFailure::Cancelled);
        assert!(harness.observer.seen().is_empty());
    }
    assert!(server.heads().is_empty());
}

#[tokio::test]
async fn cancelling_while_awaiting_response_headers_yields_cancelled_without_observer() {
    for request_level in [false, true] {
        let server = StubServer::serving(vec![Step::Hold]).await;
        let harness = Harness::new(&server.uri(), platform());
        let (request, cancel, trigger) = cancellable(request_level);
        let task = spawn_stream(&harness.gateway, request, MemorySink::default(), cancel);
        spin_until(|| !server.heads().is_empty(), "request never arrived").await;

        trigger.cancel();
        let streamed = task.await.unwrap();

        assert_eq!(streamed.failure(), GatewayFailure::Cancelled);
        assert!(harness.observer.seen().is_empty());
    }
}

#[tokio::test]
async fn cancelling_mid_body_yields_cancelled_and_keeps_only_the_delivered_bytes() {
    for request_level in [false, true] {
        let server =
            StubServer::serving(vec![chunked_head(200), chunk_frame(b"first"), Step::Hold]).await;
        let harness = Harness::new(&server.uri(), platform());
        let (request, cancel, trigger) = cancellable(request_level);
        let sink = MemorySink::default();
        let task = spawn_stream(&harness.gateway, request, sink.clone(), cancel);
        spin_until(|| sink.len() > 0, "first chunk never delivered").await;

        trigger.cancel();
        let streamed = task.await.unwrap();

        assert_eq!(streamed.failure(), GatewayFailure::Cancelled);
        assert_eq!(sink.bytes(), b"first");
        assert!(harness.observer.seen().is_empty());
    }
}

#[tokio::test]
async fn a_body_cut_short_is_an_observed_transport_failure() {
    for steps in [
        vec![chunked_head(200), chunk_frame(b"first")],
        vec![sized_head(10), Step::Send(b"short".to_vec())],
    ] {
        let server = StubServer::serving(steps).await;
        let harness = Harness::new(&server.uri(), platform());

        let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

        let failure = streamed.failure();
        assert!(
            matches!(failure, GatewayFailure::Transport(_)),
            "{failure:?}"
        );
        assert_eq!(harness.observer.seen().len(), 1);
    }
}
