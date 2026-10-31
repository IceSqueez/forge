#![allow(clippy::unwrap_used)]
mod gateway_streaming_support;
mod gateway_support;

use forge_net::{CallClass, GatewayFailure, GatewayRequest};
use gateway_streaming_support::{Step, StubServer, sized_head, stream};
use gateway_support::{Harness, chunked_ok, platform, serve_raw_once};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const REQUEST_CAP: u64 = 32;

async fn serving(body: Vec<u8>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_bytes(body))
        .mount(&server)
        .await;
    server
}

fn bytes(length: u64) -> Vec<u8> {
    vec![b'x'; usize::try_from(length).unwrap()]
}

#[tokio::test]
async fn a_request_cap_accepts_a_declared_body_at_it_and_refuses_one_byte_over() {
    for (length, refused) in [(0, false), (REQUEST_CAP, false), (REQUEST_CAP + 1, true)] {
        let server = serving(bytes(length)).await;
        let harness = Harness::new(&server.uri(), platform());

        let outcome = harness
            .gateway
            .send(GatewayRequest::get("asset").body_cap(REQUEST_CAP))
            .await;

        if refused {
            assert_eq!(
                outcome.unwrap_err(),
                GatewayFailure::ResponseTooLarge { cap: REQUEST_CAP },
                "{length}"
            );
        } else {
            assert_eq!(outcome.unwrap().body().len() as u64, length);
        }
    }
}

#[tokio::test]
async fn a_declared_length_over_the_request_cap_is_refused_before_any_body_byte_is_awaited() {
    let server = StubServer::serving(vec![
        sized_head(usize::try_from(REQUEST_CAP + 1).unwrap()),
        Step::Hold,
    ])
    .await;
    let harness = Harness::new(&server.uri(), platform());

    let failure = harness
        .fail(GatewayRequest::get("asset").body_cap(REQUEST_CAP))
        .await;

    assert_eq!(
        failure,
        GatewayFailure::ResponseTooLarge { cap: REQUEST_CAP }
    );
}

#[tokio::test]
async fn a_chunked_body_is_held_to_the_request_cap_by_cumulative_size() {
    let half = REQUEST_CAP / 2;
    for (tail, refused) in [(half, false), (half + 1, true)] {
        let base = serve_raw_once(chunked_ok(&[bytes(half), bytes(tail)])).await;
        let harness = Harness::new(&base, platform());

        let outcome = harness
            .gateway
            .send(GatewayRequest::get("asset").body_cap(REQUEST_CAP))
            .await;

        if refused {
            assert_eq!(
                outcome.unwrap_err(),
                GatewayFailure::ResponseTooLarge { cap: REQUEST_CAP }
            );
        } else {
            assert_eq!(outcome.unwrap().body().len() as u64, half + tail);
        }
    }
}

#[tokio::test]
async fn a_request_cap_above_the_class_cap_never_raises_the_class_cap() {
    let class_cap = CallClass::OAuth.body_cap();
    let server = serving(bytes(class_cap + 1)).await;
    let harness = Harness::new(&server.uri(), platform());

    let failure = harness
        .fail(
            GatewayRequest::get("token")
                .class(CallClass::OAuth)
                .body_cap(class_cap * 4),
        )
        .await;

    assert_eq!(failure, GatewayFailure::ResponseTooLarge { cap: class_cap });
}

#[tokio::test]
async fn a_streamed_send_ignores_the_request_cap_and_keeps_only_its_explicit_cap() {
    let body = bytes(REQUEST_CAP * 2);
    for (explicit_cap, delivered) in [(REQUEST_CAP * 2, true), (REQUEST_CAP * 2 - 1, false)] {
        let server = serving(body.clone()).await;
        let harness = Harness::new(&server.uri(), platform());

        let streamed = stream(
            &harness.gateway,
            GatewayRequest::get("asset").body_cap(REQUEST_CAP),
            explicit_cap,
        )
        .await;

        if delivered {
            assert_eq!(streamed.sink.bytes(), body);
        } else {
            assert_eq!(
                streamed.failure(),
                GatewayFailure::ResponseTooLarge { cap: explicit_cap }
            );
        }
    }
}
