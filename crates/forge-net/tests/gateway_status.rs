#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod gateway_support;

use std::time::Duration;

use forge_net::{
    CallClass, DEFAULT_RETRY_AFTER_SECS, FormBody, GatewayFailure, GatewayProfile, GatewayRequest,
    HeaderLookup, MAX_REMOTE_COOLDOWN, Secret, ThrottleHintFamily, ThrottleScope,
};
use gateway_support::{
    Harness, PLATFORM_NO_HINT_WAIT, chunked_ok, platform, platform_with, serve_raw_once,
};
use serde::Deserialize;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const ERROR_BODY_READ_CAP: usize = 64 * 1024;
const EXCERPT_MAX_CHARS: usize = 512;
const FAR_FUTURE_EPOCH: &str = "99999999999";

async fn serving(template: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

async fn excerpt_for(body: Vec<u8>, request: GatewayRequest) -> String {
    let server = serving(ResponseTemplate::new(400).set_body_bytes(body)).await;
    let harness = Harness::new(&server.uri(), platform());
    let failure = harness.fail(request).await;
    assert!(matches!(
        failure,
        GatewayFailure::ClientRejected { status: 400, .. }
    ));
    failure.excerpt().unwrap().as_str().to_owned()
}

fn variant(failure: &GatewayFailure) -> &'static str {
    match failure {
        GatewayFailure::Unauthorized { .. } => "Unauthorized",
        GatewayFailure::Forbidden { .. } => "Forbidden",
        GatewayFailure::NotFound { .. } => "NotFound",
        GatewayFailure::RateLimited { .. } => "RateLimited",
        GatewayFailure::ClientRejected { .. } => "ClientRejected",
        GatewayFailure::ServerError { .. } => "ServerError",
        GatewayFailure::UnexpectedStatus { .. } => "UnexpectedStatus",
        GatewayFailure::BudgetExhausted => "BudgetExhausted",
        GatewayFailure::ResponseTooLarge { .. } => "ResponseTooLarge",
        GatewayFailure::Transport(_) => "Transport",
        GatewayFailure::Cancelled => "Cancelled",
        GatewayFailure::AddressRefused(_) => "AddressRefused",
        GatewayFailure::InvalidRequest(_) => "InvalidRequest",
    }
}

#[tokio::test]
async fn success_exposes_status_case_insensitive_headers_and_body() {
    let server = serving(
        ResponseTemplate::new(201)
            .insert_header("X-Request-Id", "req-7")
            .set_body_string("created"),
    )
    .await;
    let harness = Harness::new(&server.uri(), platform());
    let response = harness.ok(GatewayRequest::post("items")).await;
    assert_eq!(response.status(), 201);
    for name in ["x-request-id", "X-REQUEST-ID", "X-Request-Id"] {
        assert_eq!(response.header_value(name), Some("req-7"), "{name}");
    }
    assert_eq!(response.header_value("x-missing"), None);
    assert_eq!(response.body(), b"created");
}

#[tokio::test]
async fn success_body_decodes_into_the_caller_type() {
    #[derive(Debug, Deserialize, PartialEq)]
    struct Item {
        id: u32,
        name: String,
    }
    let server =
        serving(ResponseTemplate::new(200).set_body_string(r#"{"id":4,"name":"x"}"#)).await;
    let harness = Harness::new(&server.uri(), platform());
    let response = harness.ok(GatewayRequest::get("items/4")).await;
    assert_eq!(
        response.json::<Item>().unwrap(),
        Item {
            id: 4,
            name: "x".to_owned()
        }
    );
}

#[tokio::test]
async fn declared_success_body_is_accepted_at_the_class_cap_and_refused_one_byte_over() {
    let cap = CallClass::OAuth.body_cap();
    let at_cap = usize::try_from(cap).unwrap();
    for (len, too_large) in [(at_cap - 1, false), (at_cap, false), (at_cap + 1, true)] {
        let server = serving(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; len])).await;
        let harness = Harness::new(&server.uri(), platform());
        let outcome = harness
            .gateway
            .send(GatewayRequest::post("token").class(CallClass::OAuth))
            .await;
        match outcome {
            Ok(response) => {
                assert!(!too_large, "len {len} should be refused");
                assert_eq!(response.body().len(), len);
            }
            Err(failure) => {
                assert!(too_large, "len {len} should be accepted");
                assert_eq!(failure, GatewayFailure::ResponseTooLarge { cap });
            }
        }
    }
}

#[tokio::test]
async fn chunked_success_body_is_capped_by_cumulative_size() {
    let cap = CallClass::OAuth.body_cap();
    let half = usize::try_from(cap / 2).unwrap();
    for (tail, too_large) in [(half, false), (half + 1, true)] {
        let base = serve_raw_once(chunked_ok(&[vec![b'a'; half], vec![b'b'; tail]])).await;
        let harness = Harness::new(&base, platform());
        let outcome = harness
            .gateway
            .send(GatewayRequest::post("token").class(CallClass::OAuth))
            .await;
        match outcome {
            Ok(response) => {
                assert!(!too_large);
                assert_eq!(response.body().len(), half + tail);
            }
            Err(failure) => {
                assert!(too_large);
                assert_eq!(failure, GatewayFailure::ResponseTooLarge { cap });
            }
        }
    }
}

#[tokio::test]
async fn body_cap_follows_the_request_class_falling_back_to_the_profile_default() {
    let oauth_cap = usize::try_from(CallClass::OAuth.body_cap()).unwrap();
    let server =
        serving(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; oauth_cap + 1])).await;
    let harness = Harness::new(&server.uri(), platform());
    let response = harness.ok(GatewayRequest::get("big")).await;
    assert_eq!(response.body().len(), oauth_cap + 1);
}

#[tokio::test]
async fn throttle_statuses_become_rate_limited_and_feed_the_limiter_once() {
    let discord = platform_with(ThrottleHintFamily::ResetAfterSeconds);
    let long_default = GatewayProfile::platform_api(
        CallClass::Interactive,
        ThrottleHintFamily::RetryAfter,
        Duration::from_secs(7200),
    );
    let secs = Duration::from_secs;
    let epoch = || platform_with(ThrottleHintFamily::ResetEpochSeconds);
    for (profile, status, headers, retry_after, scope) in [
        (
            platform(),
            429,
            vec![("Retry-After", "12")],
            secs(12),
            ThrottleScope::Bucket,
        ),
        (
            platform(),
            429,
            vec![],
            PLATFORM_NO_HINT_WAIT,
            ThrottleScope::Bucket,
        ),
        (
            platform(),
            429,
            vec![("Retry-After", "soon")],
            PLATFORM_NO_HINT_WAIT,
            ThrottleScope::Bucket,
        ),
        (
            platform(),
            429,
            vec![("Retry-After", "7200")],
            MAX_REMOTE_COOLDOWN,
            ThrottleScope::Bucket,
        ),
        (
            long_default,
            429,
            vec![],
            MAX_REMOTE_COOLDOWN,
            ThrottleScope::Bucket,
        ),
        (
            GatewayProfile::user_directed(true),
            429,
            vec![],
            secs(u64::from(DEFAULT_RETRY_AFTER_SECS)),
            ThrottleScope::Bucket,
        ),
        (
            platform(),
            503,
            vec![("Retry-After", "9")],
            secs(9),
            ThrottleScope::Bucket,
        ),
        (
            platform(),
            503,
            vec![("Retry-After", "7200")],
            MAX_REMOTE_COOLDOWN,
            ThrottleScope::Bucket,
        ),
        (
            epoch(),
            429,
            vec![("Ratelimit-Reset", FAR_FUTURE_EPOCH)],
            MAX_REMOTE_COOLDOWN,
            ThrottleScope::Bucket,
        ),
        (
            epoch(),
            503,
            vec![("Ratelimit-Reset", FAR_FUTURE_EPOCH), ("Retry-After", "9")],
            secs(9),
            ThrottleScope::Bucket,
        ),
        (
            discord,
            503,
            vec![("x-ratelimit-scope", "global"), ("Retry-After", "4")],
            secs(4),
            ThrottleScope::Bucket,
        ),
        (
            discord,
            429,
            vec![("x-ratelimit-reset-after", "2.5")],
            Duration::from_millis(2500),
            ThrottleScope::Bucket,
        ),
        (
            discord,
            429,
            vec![
                ("x-ratelimit-reset-after", "2.5"),
                ("x-ratelimit-scope", "global"),
                ("Retry-After", "4"),
            ],
            secs(4),
            ThrottleScope::Global,
        ),
    ] {
        let mut template = ResponseTemplate::new(status).set_body_string("slow down");
        for (name, value) in &headers {
            template = template.insert_header(*name, *value);
        }
        let server = serving(template).await;
        let harness = Harness::new(&server.uri(), profile);
        let failure = harness.fail(GatewayRequest::get("x")).await;
        assert!(
            matches!(
                &failure,
                GatewayFailure::RateLimited { status: s, retry_after: r, scope: sc, .. }
                    if *s == status && *r == retry_after && *sc == scope
            ),
            "{status} {headers:?}: {failure:?}"
        );
        assert_eq!(harness.limiter.observed_throttles(), vec![retry_after]);
    }
}

#[tokio::test]
async fn throttle_excerpts_are_trimmed_masked_and_capped_like_other_error_bodies() {
    let secret = "THROTTLE-SECRET";
    let long_tail = "é".repeat(EXCERPT_MAX_CHARS);
    for status in [429, 503] {
        for (body, expected) in [
            (
                format!("  quota for {secret} reached \n"),
                "quota for *** reached".to_owned(),
            ),
            (
                format!("{secret}{long_tail}"),
                format!(
                    "***{}",
                    "é".repeat(EXCERPT_MAX_CHARS - "***".chars().count())
                ),
            ),
            ("z".repeat(ERROR_BODY_READ_CAP + 1), String::new()),
        ] {
            let server = serving(
                ResponseTemplate::new(status)
                    .insert_header("Retry-After", "3")
                    .set_body_string(body),
            )
            .await;
            let harness = Harness::new(&server.uri(), platform());
            let failure = harness
                .fail(GatewayRequest::get("x").bearer(Secret::new(secret)))
                .await;
            assert!(
                matches!(&failure, GatewayFailure::RateLimited { .. }),
                "{status}: {failure:?}"
            );
            assert_eq!(
                failure.excerpt().map(|e| e.as_str()),
                Some(expected.as_str()),
                "status {status}"
            );
        }
    }
}

#[tokio::test]
async fn unhinted_503_is_a_server_error_without_limiter_feedback() {
    let server = serving(ResponseTemplate::new(503).set_body_string("maintenance")).await;
    let harness = Harness::new(&server.uri(), platform());
    let failure = harness.fail(GatewayRequest::get("x")).await;
    assert!(matches!(
        &failure,
        GatewayFailure::ServerError { status: 503, excerpt } if excerpt.as_str() == "maintenance"
    ));
    assert!(harness.limiter.observed_throttles().is_empty());
}

#[tokio::test]
async fn unusable_503_hints_are_a_server_error_without_limiter_feedback() {
    for (profile, headers) in [
        (
            platform_with(ThrottleHintFamily::ResetEpochSeconds),
            vec![("Ratelimit-Reset", FAR_FUTURE_EPOCH)],
        ),
        (platform(), vec![("Retry-After", "soon")]),
    ] {
        let mut template = ResponseTemplate::new(503).set_body_string("down");
        for (name, value) in &headers {
            template = template.insert_header(*name, *value);
        }
        let server = serving(template).await;
        let harness = Harness::new(&server.uri(), profile);
        let failure = harness.fail(GatewayRequest::get("x")).await;
        assert_eq!(variant(&failure), "ServerError", "{headers:?}");
        assert!(
            harness.limiter.observed_throttles().is_empty(),
            "{headers:?}"
        );
    }
}

#[tokio::test]
async fn non_throttle_statuses_map_to_their_failure_variant_with_an_excerpt() {
    for (status, expected, location) in [
        (401, "Unauthorized", None),
        (403, "Forbidden", None),
        (404, "NotFound", None),
        (400, "ClientRejected", None),
        (409, "ClientRejected", None),
        (422, "ClientRejected", None),
        (500, "ServerError", None),
        (502, "ServerError", None),
        (504, "ServerError", None),
        (300, "UnexpectedStatus", Some("/elsewhere")),
        (301, "UnexpectedStatus", None),
        (302, "UnexpectedStatus", None),
        (307, "UnexpectedStatus", None),
    ] {
        let mut template = ResponseTemplate::new(status).set_body_string("  reason  ");
        if let Some(location) = location {
            template = template.insert_header("Location", location);
        }
        let server = serving(template).await;
        let harness = Harness::new(&server.uri(), platform());
        let failure = harness.fail(GatewayRequest::get("x")).await;
        assert_eq!(variant(&failure), expected, "status {status}");
        assert_eq!(failure.status(), Some(status));
        assert_eq!(
            failure.excerpt().unwrap().as_str(),
            "reason",
            "status {status}"
        );
        assert!(harness.limiter.observed_throttles().is_empty());
    }
}

#[tokio::test]
async fn not_modified_is_an_unexpected_status_with_an_empty_excerpt() {
    let server = serving(ResponseTemplate::new(304)).await;
    let harness = Harness::new(&server.uri(), platform());
    let failure = harness.fail(GatewayRequest::get("x")).await;
    assert!(matches!(
        &failure,
        GatewayFailure::UnexpectedStatus { status: 304, excerpt } if excerpt.is_empty()
    ));
}

#[tokio::test]
async fn excerpt_keeps_at_most_512_characters_counting_multibyte_as_one() {
    for (len, kept) in [
        (EXCERPT_MAX_CHARS - 1, EXCERPT_MAX_CHARS - 1),
        (EXCERPT_MAX_CHARS, EXCERPT_MAX_CHARS),
        (EXCERPT_MAX_CHARS + 1, EXCERPT_MAX_CHARS),
        (600, EXCERPT_MAX_CHARS),
    ] {
        let excerpt = excerpt_for("é".repeat(len).into_bytes(), GatewayRequest::get("x")).await;
        assert_eq!(excerpt, "é".repeat(kept), "body of {len} chars");
    }
}

#[tokio::test]
async fn excerpt_is_trimmed_before_it_is_cut() {
    let body = format!(" \r\n\t{}\n", "a".repeat(EXCERPT_MAX_CHARS));
    let excerpt = excerpt_for(body.into_bytes(), GatewayRequest::get("x")).await;
    assert_eq!(excerpt, "a".repeat(EXCERPT_MAX_CHARS));
}

#[tokio::test]
async fn excerpt_decodes_invalid_utf8_lossily() {
    let excerpt = excerpt_for(vec![0xff, b'o', b'k', 0xc3], GatewayRequest::get("x")).await;
    assert_eq!(excerpt, "\u{FFFD}ok\u{FFFD}");
}

#[tokio::test]
async fn error_body_over_64_kib_gives_an_empty_excerpt() {
    for (len, empty) in [
        (ERROR_BODY_READ_CAP, false),
        (ERROR_BODY_READ_CAP + 1, true),
    ] {
        let excerpt = excerpt_for(vec![b'z'; len], GatewayRequest::get("x")).await;
        assert_eq!(excerpt.is_empty(), empty, "body of {len} bytes");
    }
}

#[tokio::test]
async fn excerpt_masks_every_request_secret_but_not_plain_values() {
    let body =
        "bearer=BEARER-1 key=APIKEY-1 query=QSECRET-1 form=FSECRET-1 plain=PLAIN-1 open=OPEN-1";
    for (body, request, expected) in [
        (
            body,
            GatewayRequest::post("x")
                .bearer(Secret::new("BEARER-1"))
                .secret_query("token", Secret::new("QSECRET-1"))
                .query("open", "OPEN-1")
                .body(
                    FormBody::new()
                        .secret_field("password", Secret::new("FSECRET-1"))
                        .field("user", "PLAIN-1"),
                ),
            "bearer=*** key=APIKEY-1 query=*** form=*** plain=PLAIN-1 open=OPEN-1",
        ),
        (
            body,
            GatewayRequest::get("x").authorization("OAuth", Secret::new("BEARER-1")),
            "bearer=*** key=APIKEY-1 query=QSECRET-1 form=FSECRET-1 plain=PLAIN-1 open=OPEN-1",
        ),
        (
            body,
            GatewayRequest::get("x").bearer(Secret::new("BEARER-1")),
            "bearer=*** key=APIKEY-1 query=QSECRET-1 form=FSECRET-1 plain=PLAIN-1 open=OPEN-1",
        ),
        (
            body,
            GatewayRequest::get("x").api_key("x-api-key", Secret::new("APIKEY-1")),
            "bearer=BEARER-1 key=*** query=QSECRET-1 form=FSECRET-1 plain=PLAIN-1 open=OPEN-1",
        ),
        (
            "abcdef",
            GatewayRequest::get("x")
                .bearer(Secret::new("abc"))
                .secret_query("t", Secret::new("abcdef")),
            "***",
        ),
        (
            "abcdef",
            GatewayRequest::get("x")
                .bearer(Secret::new("abcdef"))
                .secret_query("t", Secret::new("abc")),
            "***",
        ),
        (
            "abcdef",
            GatewayRequest::get("x")
                .secret_query("short", Secret::new("abc"))
                .secret_query("long", Secret::new("abcdef")),
            "***",
        ),
        (
            "abcdef",
            GatewayRequest::get("x")
                .secret_query("long", Secret::new("abcdef"))
                .secret_query("short", Secret::new("abc")),
            "***",
        ),
        (
            "plain reason",
            GatewayRequest::get("x").bearer(Secret::new("")),
            "plain reason",
        ),
        (
            "plain reason",
            GatewayRequest::get("x").secret_query("t", Secret::new("")),
            "plain reason",
        ),
        (
            "plain reason",
            GatewayRequest::post("x")
                .bearer(Secret::new(""))
                .body(FormBody::new().secret_field("p", Secret::new(""))),
            "plain reason",
        ),
    ] {
        let excerpt = excerpt_for(body.as_bytes().to_vec(), request).await;
        assert_eq!(excerpt, expected, "body {body:?}");
    }
}

#[tokio::test]
async fn secret_straddling_the_excerpt_cut_is_masked_before_cutting() {
    let secret = "STRADDLING-SECRET";
    let body = format!("{}{secret}", "a".repeat(EXCERPT_MAX_CHARS - 2));
    let excerpt = excerpt_for(
        body.into_bytes(),
        GatewayRequest::get("x").bearer(Secret::new(secret)),
    )
    .await;
    assert_eq!(excerpt, format!("{}**", "a".repeat(EXCERPT_MAX_CHARS - 2)));
}
