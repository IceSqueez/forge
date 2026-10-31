mod gateway_support;

use forge_net::{CallClass, GatewayRequest};
use forge_test_support::log_capture::{CapturedLine, LogRecording};
use forge_types::Secret;
use gateway_support::{Harness, platform};
use tracing::Level;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const BEARER: &str = "BearerTokenQp47";
const BODY_MARKER: &str = "invalid_form_body_marker";
const REFUSED_MESSAGE: &str = "upstream refused the request";

async fn refused_call(request: GatewayRequest) -> Vec<CapturedLine> {
    let recording = LogRecording::start(Level::TRACE);

    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(reqwest::StatusCode::BAD_REQUEST)
                .set_body_string(format!("{BODY_MARKER} {BEARER}")),
        )
        .mount(&server)
        .await;
    let harness = Harness::new(&server.uri(), platform());
    assert!(harness.gateway.send(request).await.is_err());

    recording
        .lines()
        .into_iter()
        .filter(|line| line.message() == REFUSED_MESSAGE)
        .collect()
}

#[tokio::test]
async fn a_refused_platform_call_logs_status_and_body_excerpt_at_debug() {
    let lines = refused_call(GatewayRequest::get("x").bearer(Secret::new(BEARER))).await;

    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].field("status"), Some("400"));
    assert!(lines[0].mentions(BODY_MARKER), "{:?}", lines[0]);
}

#[tokio::test]
async fn the_refusal_log_never_carries_the_request_secret() {
    let lines = refused_call(GatewayRequest::get("x").bearer(Secret::new(BEARER))).await;

    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(!lines[0].mentions(BEARER), "{:?}", lines[0]);
}

#[tokio::test]
async fn an_oauth_class_refusal_is_not_logged() {
    let lines = refused_call(
        GatewayRequest::get("x")
            .class(CallClass::OAuth)
            .bearer(Secret::new(BEARER)),
    )
    .await;

    assert!(lines.is_empty(), "{lines:?}");
}
