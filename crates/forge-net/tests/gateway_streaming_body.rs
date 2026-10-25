mod gateway_streaming_support;
mod gateway_support;

use std::time::Duration;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    BodyProgress, CallClass, GatewayFailure, GatewayRequest, HeaderLookup, RateLimitOutcome,
    STREAMING_READ_IDLE_BUDGET, TransportKind,
};
use gateway_streaming_support::{
    FRAME_GAP, MemorySink, Step, StubServer, UNLIMITED, chunk_frame, chunked_body, chunked_head,
    sized_body, sized_head, spawn_stream, spin_until, stream, stream_with, transport_kind,
};
use gateway_support::{Harness, platform};
use tokio_util::sync::CancellationToken;

const NEVER: Duration = Duration::from_secs(3600);
const JUST_UNDER_IDLE: Duration = Duration::from_secs(29);
const JUST_OVER_IDLE: Duration = Duration::from_secs(31);
const STEADY_GAP: Duration = Duration::from_secs(25);

fn progress(received: u64, declared_length: Option<u64>) -> BodyProgress {
    BodyProgress {
        received,
        declared_length,
    }
}

#[tokio::test(start_paused = true)]
async fn a_success_body_streams_into_the_sink_with_status_headers_and_byte_count() {
    let server = StubServer::serving(chunked_body(&[b"abc", b"defg", b"hi"], FRAME_GAP)).await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

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

#[tokio::test(start_paused = true)]
async fn the_cap_refuses_a_declared_or_cumulative_overrun_without_handing_the_sink_bytes_past_it() {
    let ten: &[&[u8]] = &[b"01234", b"56789"];
    let twelve: &[&[u8]] = &[b"aaaa", b"bbbb", b"cccc"];
    for (steps, cap, delivered, accepted) in [
        (sized_body(ten), 9, b"".as_slice(), false),
        (sized_body(ten), 10, b"0123456789".as_slice(), true),
        (sized_body(&[]), 0, b"".as_slice(), true),
        (
            chunked_body(twelve, FRAME_GAP),
            8,
            b"aaaabbbb".as_slice(),
            false,
        ),
        (
            chunked_body(twelve, FRAME_GAP),
            11,
            b"aaaabbbb".as_slice(),
            false,
        ),
        (
            chunked_body(twelve, FRAME_GAP),
            12,
            b"aaaabbbbcccc".as_slice(),
            true,
        ),
    ] {
        let server = StubServer::serving(steps).await;
        let harness = Harness::new(&server.uri(), platform());

        let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), cap).await;

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

#[tokio::test(start_paused = true)]
async fn a_declared_overrun_is_refused_before_any_progress_is_reported() {
    let server = StubServer::serving(sized_body(&[b"0123456789"])).await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), 9).await;

    assert!(streamed.progress.is_empty(), "{:?}", streamed.progress);
}

#[tokio::test(start_paused = true)]
async fn progress_fires_once_at_zero_and_then_after_every_delivered_chunk() {
    for (steps, expected) in [
        (
            sized_body(&[b"ab", b"cdef"]),
            vec![
                progress(0, Some(6)),
                progress(2, Some(6)),
                progress(6, Some(6)),
            ],
        ),
        (
            chunked_body(&[b"abc", b"d"], FRAME_GAP),
            vec![progress(0, None), progress(3, None), progress(4, None)],
        ),
        (sized_body(&[]), vec![progress(0, Some(0))]),
    ] {
        let server = StubServer::serving(steps).await;
        let harness = Harness::new(&server.uri(), platform());

        let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

        assert_eq!(streamed.progress, expected);
    }
}

#[tokio::test(start_paused = true)]
async fn response_headers_get_the_read_idle_budget_and_no_more() {
    for (delay, times_out) in [(JUST_UNDER_IDLE, false), (JUST_OVER_IDLE, true)] {
        let mut steps = vec![Step::Wait(delay)];
        steps.extend(sized_body(&[b"late"]));
        let server = StubServer::serving(steps).await;
        let harness = Harness::new(&server.uri(), platform());
        let started = tokio::time::Instant::now();

        let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

        if times_out {
            assert_eq!(transport_kind(&streamed.failure()), TransportKind::Timeout);
            assert_eq!(started.elapsed(), STREAMING_READ_IDLE_BUDGET);
        } else {
            assert_eq!(streamed.sink.bytes(), b"late");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn headers_that_never_arrive_time_out_and_reach_the_observer() {
    let server = StubServer::serving(vec![Step::Hold]).await;
    let harness = Harness::new(&server.uri(), platform());

    let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

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

#[tokio::test(start_paused = true)]
async fn a_gap_between_body_chunks_gets_the_read_idle_budget_and_no_more() {
    for (gap, times_out) in [(JUST_UNDER_IDLE, false), (JUST_OVER_IDLE, true)] {
        let server = StubServer::serving(chunked_body(&[b"first", b"second"], gap)).await;
        let harness = Harness::new(&server.uri(), platform());

        let streamed = stream(&harness.gateway, GatewayRequest::get("asset"), UNLIMITED).await;

        if times_out {
            assert_eq!(transport_kind(&streamed.failure()), TransportKind::Timeout);
            assert_eq!(streamed.sink.bytes(), b"first");
        } else {
            assert_eq!(streamed.sink.bytes(), b"firstsecond");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_slow_steady_body_outlasting_every_class_total_budget_still_completes() {
    let server = StubServer::serving(chunked_body(
        &[b"part-1", b"part-2", b"part-3", b"part-4"],
        STEADY_GAP,
    ))
    .await;
    let harness = Harness::new(&server.uri(), platform());
    let started = tokio::time::Instant::now();

    let streamed = stream(
        &harness.gateway,
        GatewayRequest::get("asset").class(CallClass::Media),
        UNLIMITED,
    )
    .await;

    assert_eq!(streamed.response().received(), 24);
    assert!(
        started.elapsed() > CallClass::Media.total_budget(),
        "{:?}",
        started.elapsed()
    );
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

#[tokio::test(start_paused = true)]
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
