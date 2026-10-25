#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod gateway_support;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{BodyExcerpt, ExpectedRefusal, GatewayFailure, GatewayRequest};
use gateway_support::{Harness, platform, refused_base};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const BAD_REQUEST: u16 = reqwest::StatusCode::BAD_REQUEST.as_u16();
const FORBIDDEN: u16 = reqwest::StatusCode::FORBIDDEN.as_u16();
const NOT_FOUND: u16 = reqwest::StatusCode::NOT_FOUND.as_u16();
const CONFLICT: u16 = reqwest::StatusCode::CONFLICT.as_u16();
const TOO_MANY_REQUESTS: u16 = reqwest::StatusCode::TOO_MANY_REQUESTS.as_u16();
const SERVICE_UNAVAILABLE: u16 = reqwest::StatusCode::SERVICE_UNAVAILABLE.as_u16();

async fn serving(status: u16, body: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(&server)
        .await;
    server
}

fn expecting(refusals: Vec<ExpectedRefusal>) -> GatewayRequest {
    refusals
        .into_iter()
        .fold(GatewayRequest::get("x"), GatewayRequest::expect_refusal)
}

fn status(code: u16) -> ExpectedRefusal {
    ExpectedRefusal::status(code)
}

fn marked(code: u16, marker: &str) -> ExpectedRefusal {
    ExpectedRefusal::status_with_excerpt(code, marker)
}

#[tokio::test]
async fn only_a_refusal_matching_status_and_marker_keeps_the_failure_from_the_observer() {
    let cases: [(&str, u16, &str, Vec<ExpectedRefusal>, bool); 16] = [
        ("status only", CONFLICT, "", vec![status(CONFLICT)], false),
        (
            "status only ignores the body",
            CONFLICT,
            "subscription already exists",
            vec![status(CONFLICT)],
            false,
        ),
        (
            "status only on a throttled reply",
            TOO_MANY_REQUESTS,
            "",
            vec![status(TOO_MANY_REQUESTS)],
            false,
        ),
        (
            "status only on a server error",
            SERVICE_UNAVAILABLE,
            "",
            vec![status(SERVICE_UNAVAILABLE)],
            false,
        ),
        (
            "marker in the excerpt",
            BAD_REQUEST,
            r#"{"message":"The user is not banned."}"#,
            vec![marked(BAD_REQUEST, "not banned")],
            false,
        ),
        (
            "upper-case excerpt, lower-case marker",
            BAD_REQUEST,
            "USER IS NOT BANNED",
            vec![marked(BAD_REQUEST, "not banned")],
            false,
        ),
        (
            "lower-case excerpt, upper-case marker",
            BAD_REQUEST,
            "user is not banned",
            vec![marked(BAD_REQUEST, "NOT BANNED")],
            false,
        ),
        (
            "non-ascii case folding",
            BAD_REQUEST,
            "КОРИСТУВАЧ НЕ ЗАБЛОКОВАНИЙ",
            vec![marked(BAD_REQUEST, "не заблокований")],
            false,
        ),
        (
            "second of several refusals matches",
            BAD_REQUEST,
            "Strangers are blocked",
            vec![
                marked(BAD_REQUEST, "doesn't allow"),
                marked(BAD_REQUEST, "stranger"),
            ],
            false,
        ),
        (
            "status refusal among marker refusals matches",
            FORBIDDEN,
            "",
            vec![marked(BAD_REQUEST, "not banned"), status(FORBIDDEN)],
            false,
        ),
        ("wrong status", NOT_FOUND, "", vec![status(CONFLICT)], true),
        (
            "right status, marker missing",
            BAD_REQUEST,
            "Missing required parameter user_id",
            vec![marked(BAD_REQUEST, "not banned")],
            true,
        ),
        (
            "right status, empty body",
            BAD_REQUEST,
            "",
            vec![marked(BAD_REQUEST, "not banned")],
            true,
        ),
        (
            "marker present, wrong status",
            NOT_FOUND,
            "user is not banned",
            vec![marked(BAD_REQUEST, "not banned")],
            true,
        ),
        (
            "none of several refusals matches",
            BAD_REQUEST,
            "message may not be empty",
            vec![
                marked(BAD_REQUEST, "stranger"),
                marked(BAD_REQUEST, "block"),
                status(CONFLICT),
            ],
            true,
        ),
        ("no refusal declared", CONFLICT, "", Vec::new(), true),
    ];

    for (label, reply, body, refusals, observed) in cases {
        let server = serving(reply, body).await;
        let harness = Harness::new(&server.uri(), platform());

        let failure = harness.fail(expecting(refusals)).await;

        assert_eq!(failure.status(), Some(reply), "{label}");
        assert_eq!(
            harness.observer.seen().len(),
            usize::from(observed),
            "{label}"
        );
    }
}

#[tokio::test]
async fn failures_without_a_status_reach_the_observer_despite_expected_refusals() {
    let every_refusal = || {
        vec![
            status(BAD_REQUEST),
            status(NOT_FOUND),
            status(CONFLICT),
            status(TOO_MANY_REQUESTS),
            marked(BAD_REQUEST, ""),
        ]
    };
    let server = serving(CONFLICT, "").await;
    let cases = [
        (
            "transport",
            refused_base(),
            RecordingRateLimiter::granting(),
        ),
        ("budget", server.uri(), RecordingRateLimiter::exhausted()),
    ];

    for (label, base, limiter) in cases {
        let harness = Harness::with_limiter(&base, platform(), limiter);

        let failure = harness.fail(expecting(every_refusal())).await;

        assert_eq!(failure.status(), None, "{label}");
        assert_eq!(harness.observer.seen().len(), 1, "{label}");
    }
}

#[test]
fn a_refusal_never_matches_a_failure_that_carries_no_status() {
    let refusals = [
        status(BAD_REQUEST),
        status(NOT_FOUND),
        status(TOO_MANY_REQUESTS),
        marked(BAD_REQUEST, ""),
    ];
    let statusless = [
        GatewayFailure::Cancelled,
        GatewayFailure::BudgetExhausted { retry_after: None },
        GatewayFailure::ResponseTooLarge { cap: 1 },
    ];

    for failure in &statusless {
        for refusal in &refusals {
            assert!(!refusal.matches(failure), "{refusal:?} matched {failure:?}");
        }
    }
}

#[test]
fn a_status_refusal_matches_an_empty_excerpt_that_a_marker_refusal_does_not() {
    let failure = GatewayFailure::NotFound {
        excerpt: BodyExcerpt::default(),
    };

    assert_eq!(
        (
            status(NOT_FOUND).matches(&failure),
            marked(NOT_FOUND, "gone").matches(&failure)
        ),
        (true, false)
    );
}

#[tokio::test]
async fn an_expected_refusal_leaves_the_returned_failure_unchanged() {
    let body = r#"{"error":"Bad Request","status":400,"message":"The user is not banned."}"#;
    let server = serving(BAD_REQUEST, body).await;
    let harness = Harness::new(&server.uri(), platform());

    let unexpected = harness.fail(GatewayRequest::get("x")).await;
    let expected = harness
        .fail(GatewayRequest::get("x").expect_refusal(marked(BAD_REQUEST, "NOT BANNED")))
        .await;

    assert_eq!(expected, unexpected);
}
