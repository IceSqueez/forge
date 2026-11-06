#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "test fixtures assert directly"
)]
mod gateway_grpc_support;
mod gateway_support;

use std::time::Duration;

use forge_net::{
    BodyExcerpt, GatewayFailure, GrpcItem, GrpcRequest, MAX_REMOTE_COOLDOWN, ThrottleScope,
    TransportKind,
};
use forge_types::Secret;
use gateway_grpc_support::{
    GRPC, GRPC_DETAILS, GRPC_MESSAGE, H2Server, Pairs, Seen, Step, details_bin, drain, end_of,
    error_info, failure_of, grpc_head, head_with, kind_of, padded_details_bin, retry_info,
    status_pairs, status_proto, trailers_only,
};
use gateway_support::{Harness, PLATFORM_NO_HINT_WAIT, platform};
use tokio_util::sync::CancellationToken;

const PATH: &str = "/forge.test.Feed/Stream";
const CAP: u64 = 1024;
const HANG_GUARD: Duration = Duration::from_secs(10);
const TOKEN: &str = "tok-grpc-7f3a";
const RESOURCE_EXHAUSTED: &str = "8";
const PERMISSION_DENIED: &str = "7";
const UNAVAILABLE: &str = "14";
const RETRY_AFTER: &str = "retry-after";
const TRAILERS: &str = "trailers";
const HEADERS: &str = "headers";

#[derive(Debug, PartialEq)]
enum Outcome {
    Clean,
    Failed(GatewayFailure),
    Transport(TransportKind),
}

fn outcome_of(end: Result<(), GatewayFailure>) -> Outcome {
    match end {
        Ok(()) => Outcome::Clean,
        Err(GatewayFailure::Transport(transport)) => Outcome::Transport(transport.kind()),
        Err(failure) => Outcome::Failed(failure),
    }
}

fn plain(status: u16) -> Outcome {
    let excerpt = BodyExcerpt::default();
    Outcome::Failed(match status {
        401 => GatewayFailure::Unauthorized { excerpt },
        403 => GatewayFailure::Forbidden { excerpt },
        404 => GatewayFailure::NotFound { excerpt },
        status if status < 500 => GatewayFailure::ClientRejected { status, excerpt },
        status => GatewayFailure::ServerError { status, excerpt },
    })
}

fn call() -> GrpcRequest {
    GrpcRequest::new(PATH, b"request".to_vec(), CAP).read_idle(HANG_GUARD)
}

async fn end_with(request: GrpcRequest, steps: Vec<Step>) -> (Harness, Vec<GrpcItem>) {
    let server = H2Server::serving(steps).await;
    let harness = Harness::new(&server.uri(), platform());
    let mut stream = harness
        .gateway
        .open_grpc_stream(request, CancellationToken::new())
        .await
        .unwrap();
    let items = drain(&mut stream).await;
    (harness, items)
}

async fn failure_with(request: GrpcRequest, trailers: Pairs) -> GatewayFailure {
    let (_harness, items) = end_with(request, vec![grpc_head(), Step::Trailers(trailers)]).await;
    failure_of(&items)
}

fn exhausted_with(extra: Pairs) -> Pairs {
    let mut pairs = status_pairs(RESOURCE_EXHAUSTED);
    pairs.extend(extra);
    pairs
}

fn rate_limited(failure: &GatewayFailure) -> (Duration, Option<String>) {
    match failure {
        GatewayFailure::RateLimited {
            status: 429,
            retry_after,
            scope: ThrottleScope::Bucket,
            excerpt,
        } => (*retry_after, excerpt.reason().map(str::to_owned)),
        other => panic!("expected a bucket rate limit, got {other:?}"),
    }
}

fn steps_for(seen: &Seen) -> Vec<Step> {
    let mut parts = seen.path.rsplit('/');
    let code = parts.next().unwrap().to_owned();
    match parts.next() {
        Some(TRAILERS) => vec![grpc_head(), Step::Trailers(status_pairs(&code))],
        Some(HEADERS) => vec![trailers_only(status_pairs(&code))],
        other => panic!("unexpected placement {other:?}"),
    }
}

#[tokio::test]
async fn every_call_status_code_maps_to_its_gateway_outcome_in_trailers_and_trailers_only() {
    let server = H2Server::scripted(steps_for).await;
    let harness = Harness::new(&server.uri(), platform());
    let exhausted = Outcome::Failed(GatewayFailure::RateLimited {
        status: 429,
        retry_after: PLATFORM_NO_HINT_WAIT,
        scope: ThrottleScope::Bucket,
        excerpt: BodyExcerpt::default(),
    });
    let unimplemented = Outcome::Failed(GatewayFailure::ProtocolNotOffered {
        excerpt: BodyExcerpt::default(),
    });
    let table = [
        (0, Outcome::Clean),
        (1, Outcome::Transport(TransportKind::Other)),
        (2, plain(500)),
        (3, plain(400)),
        (4, Outcome::Transport(TransportKind::Timeout)),
        (5, plain(404)),
        (6, plain(409)),
        (7, plain(403)),
        (8, exhausted),
        (9, plain(400)),
        (10, plain(409)),
        (11, plain(400)),
        (12, unimplemented),
        (13, plain(500)),
        (14, Outcome::Transport(TransportKind::Other)),
        (15, plain(500)),
        (16, plain(401)),
        (17, plain(500)),
        (99, plain(500)),
    ];
    for placement in [TRAILERS, HEADERS] {
        for (code, expected) in &table {
            let request = GrpcRequest::new(format!("/codes/{placement}/{code}"), Vec::new(), CAP)
                .read_idle(HANG_GUARD);
            let mut stream = harness
                .gateway
                .open_grpc_stream(request, CancellationToken::new())
                .await
                .unwrap();

            let items = drain(&mut stream).await;

            assert_eq!(items.len(), 1, "{placement} {code}: {items:?}");
            assert_eq!(
                outcome_of(end_of(&items)),
                *expected,
                "{placement} code {code}"
            );
        }
    }
}

#[tokio::test]
async fn grpc_message_is_percent_decoded_before_the_bearer_is_masked() {
    let request = call().bearer(Secret::new(TOKEN));
    let mut trailers = status_pairs(PERMISSION_DENIED);
    trailers.push((
        GRPC_MESSAGE,
        "token%20tok%2Dgrpc%2D7f3a%20refused%20%E2%9C%93".to_owned(),
    ));

    let failure = failure_with(request, trailers).await;

    assert_eq!(
        failure.excerpt().unwrap().as_str(),
        "token *** refused \u{2713}"
    );
}

#[tokio::test]
async fn excerpt_prefers_grpc_message_and_falls_back_to_the_details_message() {
    let details = details_bin(&status_proto(Some("from details"), &[]));
    for (message, expected) in [
        (Some("from%20header"), "from header"),
        (None, "from details"),
    ] {
        let mut trailers = status_pairs(PERMISSION_DENIED);
        trailers.push((GRPC_DETAILS, details.clone()));
        if let Some(message) = message {
            trailers.push((GRPC_MESSAGE, message.to_owned()));
        }

        let failure = failure_with(call(), trailers).await;

        assert_eq!(failure.excerpt().unwrap().as_str(), expected);
    }
}

#[tokio::test]
async fn details_message_carrying_the_bearer_is_masked() {
    let details = details_bin(&status_proto(Some("bad token tok-grpc-7f3a"), &[]));
    let mut trailers = status_pairs(PERMISSION_DENIED);
    trailers.push((GRPC_DETAILS, details));

    let failure = failure_with(call().bearer(Secret::new(TOKEN)), trailers).await;

    assert_eq!(failure.excerpt().unwrap().as_str(), "bad token ***");
}

#[tokio::test]
async fn quota_reasons_reach_the_excerpt_with_and_without_base64_padding() {
    for reason in ["quotaExceeded", "rateLimitExceeded"] {
        let status = status_proto(Some("quota"), &[error_info(reason)]);
        let padded = padded_details_bin(&status);
        assert!(
            padded.ends_with('='),
            "fixture must carry padding: {padded}"
        );
        for encoded in [padded, details_bin(&status)] {
            let failure = failure_with(
                call(),
                exhausted_with(vec![(GRPC_DETAILS, encoded.clone())]),
            )
            .await;

            assert_eq!(
                rate_limited(&failure).1.as_deref(),
                Some(reason),
                "{encoded}"
            );
        }
    }
}

#[tokio::test]
async fn retry_hint_prefers_retry_info_then_retry_after_then_the_profile_default() {
    let retry_details = details_bin(&status_proto(None, &[retry_info(7, 500_000_000)]));
    for (extra, expected) in [
        (
            vec![
                (GRPC_DETAILS, retry_details.clone()),
                (RETRY_AFTER, "30".to_owned()),
            ],
            Duration::from_millis(7_500),
        ),
        (
            vec![(RETRY_AFTER, "30".to_owned())],
            Duration::from_secs(30),
        ),
        (Vec::new(), PLATFORM_NO_HINT_WAIT),
    ] {
        let failure = failure_with(call(), exhausted_with(extra)).await;

        assert_eq!(rate_limited(&failure).0, expected);
    }
}

#[tokio::test]
async fn retry_info_beyond_the_remote_cooldown_ceiling_is_capped() {
    let details = details_bin(&status_proto(None, &[retry_info(10_000_000, 0)]));

    let failure = failure_with(call(), exhausted_with(vec![(GRPC_DETAILS, details)])).await;

    assert_eq!(rate_limited(&failure).0, MAX_REMOTE_COOLDOWN);
}

#[tokio::test]
async fn unreadable_details_leave_the_failure_classified_by_code_alone() {
    let whole = status_proto(None, &[error_info("quotaExceeded"), retry_info(9, 0)]);
    for encoded in [
        "%%not-base64%%".to_owned(),
        details_bin(&whole[..whole.len() - 3]),
        details_bin(&[0x0b, 0x01]),
    ] {
        let mut extra = vec![(GRPC_DETAILS, encoded.clone())];
        extra.push((GRPC_MESSAGE, "slow down".to_owned()));

        let failure = failure_with(call(), exhausted_with(extra)).await;

        assert_eq!(
            rate_limited(&failure),
            (PLATFORM_NO_HINT_WAIT, None),
            "{encoded}"
        );
        assert_eq!(failure.excerpt().unwrap().as_str(), "slow down");
    }
}

#[tokio::test]
async fn reason_with_invalid_characters_or_carrying_the_secret_is_dropped() {
    let secret = "tokgrpc7f3a";
    for reason in ["quota exceeded", "quota.tokgrpc7f3a"] {
        let details = details_bin(&status_proto(None, &[error_info(reason)]));
        let request = call().bearer(Secret::new(secret));

        let failure = failure_with(request, exhausted_with(vec![(GRPC_DETAILS, details)])).await;

        assert_eq!(rate_limited(&failure).1, None, "{reason}");
    }
}

#[tokio::test]
async fn resource_exhausted_end_leaves_the_shared_limiter_to_the_caller() {
    let details = details_bin(&status_proto(None, &[retry_info(12, 0)]));
    let (harness, items) = end_with(
        call(),
        vec![
            grpc_head(),
            Step::Trailers(exhausted_with(vec![(GRPC_DETAILS, details)])),
        ],
    )
    .await;

    assert_eq!(rate_limited(&failure_of(&items)).0, Duration::from_secs(12));
    assert_eq!(harness.limiter.observed_throttles(), Vec::<Duration>::new());
}

async fn open_failure(steps: Vec<Step>) -> GatewayFailure {
    let server = H2Server::serving(steps).await;
    let harness = Harness::new(&server.uri(), platform());
    harness
        .gateway
        .open_grpc_stream(call().bearer(Secret::new(TOKEN)), CancellationToken::new())
        .await
        .unwrap_err()
}

#[tokio::test]
async fn answers_without_a_grpc_content_type_or_with_404_are_protocol_not_offered() {
    for (status, content_type) in [
        (200, "text/html"),
        (200, "application/grpc-web"),
        (200, "application/grpcx"),
        (404, "text/plain"),
        (404, GRPC),
    ] {
        let failure = open_failure(vec![
            head_with(status, content_type, Vec::new(), false),
            Step::Data(b"<html>not here</html>".to_vec()),
            Step::Finish,
        ])
        .await;

        assert!(
            matches!(failure, GatewayFailure::ProtocolNotOffered { .. }),
            "{status} {content_type}: {failure:?}"
        );
    }
}

#[tokio::test]
async fn protocol_not_offered_excerpt_masks_the_bearer() {
    let failure = open_failure(vec![
        head_with(200, "text/html", Vec::new(), false),
        Step::Data(format!("<p>sign in {TOKEN}</p>").into_bytes()),
        Step::Finish,
    ])
    .await;

    assert_eq!(failure.excerpt().unwrap().as_str(), "<p>sign in ***</p>");
}

#[tokio::test]
async fn grpc_content_type_variants_are_accepted() {
    for content_type in [
        "application/grpc+proto",
        "application/grpc;charset=utf-8",
        "Application/GRPC",
    ] {
        let (_harness, items) = end_with(
            call(),
            vec![
                head_with(200, content_type, Vec::new(), false),
                Step::Trailers(status_pairs("0")),
            ],
        )
        .await;

        assert_eq!(end_of(&items), Ok(()), "{content_type}");
    }
}

#[tokio::test]
async fn refusals_without_grpc_keep_the_rest_classification() {
    for (status, expected) in [(401, plain(401)), (403, plain(403)), (503, plain(503))] {
        let failure = open_failure(vec![head_with(status, "text/plain", Vec::new(), true)]).await;

        assert_eq!(Outcome::Failed(failure), expected, "{status}");
    }
    let throttled = open_failure(vec![head_with(
        429,
        "text/plain",
        vec![(RETRY_AFTER, "4".to_owned())],
        true,
    )])
    .await;
    assert!(
        matches!(
            throttled,
            GatewayFailure::RateLimited {
                status: 429,
                retry_after,
                ..
            } if retry_after == Duration::from_secs(4)
        ),
        "{throttled:?}"
    );
}

#[tokio::test]
async fn non_ok_http_status_with_a_grpc_status_is_read_as_a_call_status() {
    let (_harness, items) = end_with(
        call(),
        vec![head_with(503, GRPC, status_pairs(UNAVAILABLE), true)],
    )
    .await;

    assert_eq!(kind_of(&failure_of(&items)), TransportKind::Other);
}
