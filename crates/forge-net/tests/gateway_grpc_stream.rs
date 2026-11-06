#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
mod gateway_grpc_support;
mod gateway_streaming_support;
mod gateway_support;

use std::time::Duration;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    ExpectedRefusal, GatewayFailure, GatewayRequest, GrpcItem, GrpcRequest, GrpcStream, Method,
    RateLimitOutcome, TransportKind,
};
use forge_types::Secret;
use gateway_grpc_support::{
    GRPC, H2Server, STATUS_OK, Step, drain, end_of, ending_with, failure_of, frame,
    frame_with_flag, grpc_head, head_with, kind_of, ok_trailers, status_pairs, trailers_only,
};
use gateway_streaming_support::{StubServer, UNLIMITED, sized_body, stream};
use gateway_support::{Harness, platform};
use http::Version;
use tokio_util::sync::CancellationToken;

const PATH: &str = "/forge.test.Feed/Stream";
const CAP: u64 = 1024;
const HANG_GUARD: Duration = Duration::from_secs(10);
const SHORT_IDLE: Duration = Duration::from_millis(100);
const MAX_IDLE_ATTEMPTS: u32 = 6;
const TOKEN: &str = "tok-grpc-7f3a";
const LABEL: &str = "feed.stream";
const STATUS_INTERNAL: &str = "13";
const STATUS_PERMISSION_DENIED: &str = "7";
const HTTP_FORBIDDEN: u16 = 403;
const HTTP_TEMPORARY_REDIRECT: u16 = 307;

fn call() -> GrpcRequest {
    GrpcRequest::new(PATH, b"request".to_vec(), CAP).read_idle(HANG_GUARD)
}

async fn served(steps: Vec<Step>) -> (H2Server, Harness) {
    let server = H2Server::serving(steps).await;
    let harness = Harness::new(&server.uri(), platform());
    (server, harness)
}

async fn open(harness: &Harness, request: GrpcRequest) -> GrpcStream {
    harness
        .gateway
        .open_grpc_stream(request, CancellationToken::new())
        .await
        .unwrap()
}

async fn items_of(steps: Vec<Step>) -> Vec<GrpcItem> {
    let (_server, harness) = served(steps).await;
    drain(&mut open(&harness, call()).await).await
}

fn message(bytes: &[u8]) -> GrpcItem {
    GrpcItem::Message(bytes.to_vec())
}

#[tokio::test]
async fn messages_arrive_in_order_then_one_end_then_none() {
    let (_server, harness) = served(vec![
        grpc_head(),
        Step::Data(frame(b"alpha")),
        Step::Data(frame(b"beta")),
        ok_trailers(),
    ])
    .await;
    let mut stream = open(&harness, call()).await;

    let items = drain(&mut stream).await;

    assert_eq!(
        items,
        vec![message(b"alpha"), message(b"beta"), GrpcItem::End(Ok(()))]
    );
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn several_messages_in_one_data_frame_are_delivered_separately_in_order() {
    let mut packed = frame(b"one");
    packed.extend(frame(b""));
    packed.extend(frame(b"three"));

    let items = items_of(vec![grpc_head(), Step::Data(packed), ok_trailers()]).await;

    assert_eq!(
        items,
        vec![
            message(b"one"),
            message(b""),
            message(b"three"),
            GrpcItem::End(Ok(()))
        ]
    );
}

#[tokio::test]
async fn a_message_split_across_data_frames_is_reassembled() {
    let whole = frame(b"split-message");

    let items = items_of(vec![
        grpc_head(),
        Step::Data(whole[..3].to_vec()),
        Step::Data(whole[3..9].to_vec()),
        Step::Data(whole[9..].to_vec()),
        ok_trailers(),
    ])
    .await;

    assert_eq!(
        items,
        vec![message(b"split-message"), GrpcItem::End(Ok(()))]
    );
}

#[tokio::test]
async fn trailers_only_ok_ends_the_stream_without_messages() {
    let items = items_of(vec![trailers_only(status_pairs(STATUS_OK))]).await;

    assert_eq!(items, vec![GrpcItem::End(Ok(()))]);
}

#[tokio::test]
async fn message_exactly_at_the_cap_is_delivered() {
    let at_cap = vec![b'x'; usize::try_from(CAP).unwrap()];

    let items = items_of(vec![grpc_head(), Step::Data(frame(&at_cap)), ok_trailers()]).await;

    assert_eq!(
        items,
        vec![GrpcItem::Message(at_cap), GrpcItem::End(Ok(()))]
    );
}

#[tokio::test]
async fn message_declared_one_over_the_cap_fails_before_its_payload_arrives() {
    let mut prefix = vec![0_u8];
    prefix.extend_from_slice(&u32::try_from(CAP + 1).unwrap().to_be_bytes());

    let items = items_of(vec![grpc_head(), Step::Data(prefix), Step::Hold]).await;

    assert_eq!(
        items,
        vec![GrpcItem::End(Err(GatewayFailure::ResponseTooLarge {
            cap: CAP
        }))]
    );
}

#[tokio::test]
async fn compressed_or_unknown_frame_flags_end_with_a_protocol_failure() {
    for flag in [1_u8, 2, 0x80] {
        let items = items_of(vec![
            grpc_head(),
            Step::Data(frame_with_flag(flag, b"payload")),
            ok_trailers(),
        ])
        .await;

        assert_eq!(items.len(), 1, "flag {flag}: {items:?}");
        assert_eq!(
            kind_of(&failure_of(&items)),
            TransportKind::Protocol,
            "flag {flag}"
        );
    }
}

#[tokio::test]
async fn body_ending_inside_a_message_is_a_protocol_failure() {
    let whole = frame(b"cut-short");
    for closing in [Step::Finish, ok_trailers()] {
        let items = items_of(vec![grpc_head(), Step::Data(whole[..7].to_vec()), closing]).await;

        assert_eq!(items.len(), 1, "{items:?}");
        assert_eq!(kind_of(&failure_of(&items)), TransportKind::Protocol);
    }
}

#[tokio::test]
async fn response_without_a_call_status_is_a_protocol_failure() {
    for closing in [
        Step::Finish,
        Step::Trailers(vec![("x-trace", "1".to_owned())]),
    ] {
        let items = items_of(vec![grpc_head(), Step::Data(frame(b"kept")), closing]).await;

        assert_eq!(items[0], message(b"kept"));
        assert_eq!(items.len(), 2, "{items:?}");
        assert_eq!(kind_of(&failure_of(&items)), TransportKind::Protocol);
    }
}

#[tokio::test]
async fn malformed_call_status_is_a_protocol_failure_in_trailers_and_in_headers() {
    for raw in ["abc", "-1", "", "4294967296", "1.0"] {
        for steps in [
            vec![grpc_head(), Step::Trailers(status_pairs(raw))],
            vec![trailers_only(status_pairs(raw))],
        ] {
            let items = items_of(steps).await;

            assert_eq!(
                kind_of(&failure_of(&items)),
                TransportKind::Protocol,
                "status {raw:?}"
            );
        }
    }
}

#[tokio::test]
async fn response_head_stalled_past_read_idle_times_out() {
    let (_server, harness) = served(vec![Step::Hold]).await;

    let failure = harness
        .gateway
        .open_grpc_stream(call().read_idle(SHORT_IDLE), CancellationToken::new())
        .await
        .unwrap_err();

    assert_eq!(kind_of(&failure), TransportKind::Timeout);
}

#[tokio::test]
async fn body_stalled_past_read_idle_times_out() {
    let (_server, harness) = served(vec![grpc_head(), Step::Hold]).await;
    let mut budget = SHORT_IDLE;
    let mut attempts = 0;
    let mut stream = loop {
        attempts += 1;
        match harness
            .gateway
            .open_grpc_stream(call().read_idle(budget), CancellationToken::new())
            .await
        {
            Ok(stream) => break stream,
            Err(failure) if attempts < MAX_IDLE_ATTEMPTS => {
                assert_eq!(kind_of(&failure), TransportKind::Timeout);
                budget *= 2;
            }
            Err(failure) => panic!("the response head never arrived: {failure:?}"),
        }
    };

    let items = drain(&mut stream).await;

    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(kind_of(&failure_of(&items)), TransportKind::Timeout);
}

#[tokio::test]
async fn cancel_while_opening_ends_with_cancelled_and_is_not_reported() {
    let (server, harness) = served(vec![Step::Hold]).await;
    let cancel = CancellationToken::new();

    let (opened, ()) = tokio::join!(
        harness.gateway.open_grpc_stream(call(), cancel.clone()),
        async {
            server.request_arrived().await;
            cancel.cancel();
        }
    );

    assert_eq!(opened.unwrap_err(), GatewayFailure::Cancelled);
    assert!(harness.observer.seen().is_empty());
}

#[tokio::test]
async fn cancel_while_reading_ends_with_cancelled_and_is_not_reported() {
    let (_server, harness) = served(vec![grpc_head(), Step::Hold]).await;
    let cancel = CancellationToken::new();
    let mut stream = harness
        .gateway
        .open_grpc_stream(call(), cancel.clone())
        .await
        .unwrap();

    let (item, ()) = tokio::join!(stream.next(), async {
        tokio::task::yield_now().await;
        cancel.cancel();
    });

    assert_eq!(item, Some(GrpcItem::End(Err(GatewayFailure::Cancelled))));
    assert!(harness.observer.seen().is_empty());
}

#[tokio::test]
async fn failed_end_is_reported_once_with_the_call_context() {
    let (_server, harness) = served(ending_with(STATUS_INTERNAL)).await;
    let mut stream = open(&harness, call().label(LABEL)).await;

    let items = drain(&mut stream).await;
    assert_eq!(stream.next().await, None);

    let seen = harness.observer.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    let (failure, context) = &seen[0];
    assert_eq!(*failure, failure_of(&items));
    assert_eq!((context.method, context.label), (Method::Post, Some(LABEL)));
}

#[tokio::test]
async fn successful_call_is_not_reported() {
    let (_server, harness) =
        served(vec![grpc_head(), Step::Data(frame(b"fine")), ok_trailers()]).await;

    let items = drain(&mut open(&harness, call()).await).await;

    assert_eq!(end_of(&items), Ok(()));
    assert!(harness.observer.seen().is_empty());
}

#[tokio::test]
async fn expected_refusal_end_is_not_reported() {
    let (_server, harness) = served(ending_with(STATUS_PERMISSION_DENIED)).await;
    let request = call().expect_refusal(ExpectedRefusal::status(HTTP_FORBIDDEN));

    let items = drain(&mut open(&harness, request).await).await;

    assert!(matches!(
        failure_of(&items),
        GatewayFailure::Forbidden { .. }
    ));
    assert!(harness.observer.seen().is_empty());
}

#[tokio::test]
async fn request_head_is_an_http2_post_with_grpc_content_type_trailers_and_bearer() {
    let (server, harness) = served(ending_with(STATUS_OK)).await;
    let request = call()
        .bearer(Secret::new(TOKEN))
        .metadata("x-goog-request-params", "chat=abc");

    drain(&mut open(&harness, request).await).await;

    let seen = server.seen();
    assert_eq!(seen.len(), 1);
    let head = &seen[0];
    assert_eq!(head.version, Version::HTTP_2);
    assert_eq!(head.method, http::Method::POST);
    assert_eq!(head.path, PATH);
    assert_eq!(head.header("content-type"), Some(GRPC));
    assert_eq!(head.header("te"), Some("trailers"));
    assert_eq!(
        head.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(head.header("x-goog-request-params"), Some("chat=abc"));
}

#[tokio::test]
async fn request_body_is_one_uncompressed_length_prefixed_message() {
    let (server, harness) = served(ending_with(STATUS_OK)).await;

    drain(
        &mut open(
            &harness,
            GrpcRequest::new(PATH, b"\x0a\x03abc".to_vec(), CAP),
        )
        .await,
    )
    .await;

    assert_eq!(server.seen()[0].body, b"\x00\x00\x00\x00\x05\x0a\x03abc");
}

#[tokio::test]
async fn redirect_answer_is_not_followed() {
    let server = H2Server::serving(vec![head_with(
        HTTP_TEMPORARY_REDIRECT,
        "text/plain",
        vec![("location", "/elsewhere".to_owned())],
        true,
    )])
    .await;
    let harness = Harness::new(&server.uri(), platform());

    let failure = harness
        .gateway
        .open_grpc_stream(call(), CancellationToken::new())
        .await
        .unwrap_err();

    assert!(
        matches!(
            failure,
            GatewayFailure::UnexpectedStatus {
                status: HTTP_TEMPORARY_REDIRECT,
                ..
            }
        ),
        "{failure:?}"
    );
    let paths: Vec<String> = server.seen().into_iter().map(|seen| seen.path).collect();
    assert_eq!(paths, vec![PATH.to_owned()]);
}

#[tokio::test]
async fn one_limiter_acquire_per_call_regardless_of_message_count() {
    let (_server, harness) = served(vec![
        grpc_head(),
        Step::Data(frame(b"1")),
        Step::Data(frame(b"2")),
        Step::Data(frame(b"3")),
        ok_trailers(),
    ])
    .await;

    let items = drain(&mut open(&harness, call()).await).await;

    assert_eq!(items.len(), 4);
    assert_eq!(harness.limiter.acquire_count(), 1);
}

#[tokio::test]
async fn denied_budget_fails_before_any_request_is_sent() {
    let throttled = Duration::from_secs(3);
    for (limiter, request, expected) in [
        (RecordingRateLimiter::exhausted(), call(), None),
        (
            RecordingRateLimiter::answering(Ok(RateLimitOutcome::Throttled {
                wait_for: throttled,
            })),
            call().without_limiter_wait(),
            Some(throttled),
        ),
    ] {
        let server = H2Server::serving(ending_with(STATUS_OK)).await;
        let harness = Harness::with_limiter(&server.uri(), platform(), limiter);

        let failure = harness
            .gateway
            .open_grpc_stream(request, CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(
            failure,
            GatewayFailure::BudgetExhausted {
                retry_after: expected
            }
        );
        assert!(server.seen().is_empty());
    }
}

#[tokio::test]
async fn debug_output_never_shows_the_bearer_or_message_bytes() {
    let (_server, harness) = served(vec![grpc_head(), Step::Hold]).await;
    let request = GrpcRequest::new(PATH, b"payload-SENSITIVE".to_vec(), CAP)
        .bearer(Secret::new(TOKEN))
        .read_idle(HANG_GUARD);
    let request_debug = format!("{request:?}");

    let stream = open(&harness, request).await;
    let stream_debug = format!("{stream:?}");

    for shown in [request_debug, stream_debug] {
        assert!(!shown.contains(TOKEN), "{shown}");
        assert!(!shown.contains("payload-SENSITIVE"), "{shown}");
    }
}

#[tokio::test]
async fn plain_calls_stay_on_http1_after_a_grpc_call_on_the_same_gateway() {
    let server = StubServer::serving(sized_body(&[b"plain"])).await;
    let harness = Harness::new(&server.uri(), platform());
    let grpc = harness
        .gateway
        .open_grpc_stream(call(), CancellationToken::new())
        .await;
    assert!(grpc.is_err());

    harness.ok(GatewayRequest::get("plain")).await;
    stream(&harness.gateway, GatewayRequest::get("plain"), UNLIMITED)
        .await
        .response();

    let heads = server.heads();
    assert!(heads.iter().any(|head| head.starts_with("PRI * HTTP/2.0")));
    let plain: Vec<&String> = heads
        .iter()
        .filter(|head| !head.starts_with("PRI "))
        .collect();
    assert_eq!(plain.len(), 2, "{heads:?}");
    for head in plain {
        assert!(head.starts_with("GET /plain HTTP/1.1\r\n"), "{head}");
    }
}
