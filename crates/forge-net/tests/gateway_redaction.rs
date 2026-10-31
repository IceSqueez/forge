mod gateway_support;

use std::collections::BTreeSet;

use forge_net::test_support::RecordingRateLimiter;
use forge_net::{
    CallClass, Credential, DecodeCategory, FormBody, GatewayFailure, GatewayProfile,
    GatewayRequest, Method, MultipartBody, RequestBody,
};
use forge_types::Secret;
use gateway_support::{Harness, platform, refused_base, serve_raw_once};
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const PATH_TOKEN: &str = "PathTokenZq81";
const BASE_TOKEN: &str = "BaseTokenXv27";
const QUERY_SECRET: &str = "QuerySecretMm40";
const QUERY_PLAIN: &str = "QueryPlainHb66";
const HEADER_VALUE: &str = "HeaderValueKd13";
const BEARER: &str = "BearerTokenWr59";
const API_KEY: &str = "ApiKeyValueNs72";
const FORM_SECRET: &str = "FormSecretLc05";
const JSON_VALUE: &str = "JsonValueTe38";
const SENSITIVE: [&str; 9] = [
    PATH_TOKEN,
    BASE_TOKEN,
    QUERY_SECRET,
    QUERY_PLAIN,
    HEADER_VALUE,
    BEARER,
    API_KEY,
    FORM_SECRET,
    JSON_VALUE,
];

fn assert_redacted(rendered: &str, what: &str) {
    for secret in SENSITIVE {
        assert!(
            !rendered.contains(secret),
            "{what} leaked {secret}: {rendered}"
        );
    }
}

fn webhook_request() -> GatewayRequest {
    GatewayRequest::post(format!("webhooks/1234/{PATH_TOKEN}"))
        .query("thread", QUERY_PLAIN)
        .secret_query("wait", Secret::new(QUERY_SECRET))
        .header("x-trace", HEADER_VALUE)
        .bearer(Secret::new(BEARER))
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
        GatewayFailure::BudgetExhausted { .. } => "BudgetExhausted",
        GatewayFailure::ResponseTooLarge { .. } => "ResponseTooLarge",
        GatewayFailure::Transport(_) => "Transport",
        GatewayFailure::Cancelled => "Cancelled",
        GatewayFailure::AddressRefused(_) => "AddressRefused",
        GatewayFailure::InvalidRequest(_) => "InvalidRequest",
    }
}

async fn serving(template: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

async fn failure_from(
    base: &str,
    profile: GatewayProfile,
    limiter: RecordingRateLimiter,
    request: GatewayRequest,
) -> GatewayFailure {
    let base = format!("{base}/api/webhooks/1/{BASE_TOKEN}");
    Harness::with_limiter(&base, profile, limiter)
        .fail(request)
        .await
}

#[tokio::test]
async fn request_response_body_credential_and_gateway_debug_never_print_values() {
    let server = serving(
        ResponseTemplate::new(reqwest::StatusCode::OK)
            .insert_header("set-cookie", HEADER_VALUE)
            .set_body_string(JSON_VALUE),
    )
    .await;
    let gateway_base = format!("{}/api/webhooks/1/{BASE_TOKEN}", server.uri());
    let harness = Harness::new(&gateway_base, platform());
    let response = harness.ok(GatewayRequest::get("x")).await;
    let rendered = [
        ("request", format!("{:?}", webhook_request())),
        (
            "request with api key and form",
            format!(
                "{:?}",
                GatewayRequest::post(PATH_TOKEN)
                    .api_key("x-api-key", Secret::new(API_KEY))
                    .body(
                        FormBody::new()
                            .secret_field("password", Secret::new(FORM_SECRET))
                            .field("user", QUERY_PLAIN)
                    )
            ),
        ),
        (
            "json body",
            format!(
                "{:?}",
                RequestBody::json(&json!({ "content": JSON_VALUE })).unwrap()
            ),
        ),
        (
            "raw body",
            format!("{:?}", RequestBody::raw("text/plain", JSON_VALUE)),
        ),
        (
            "multipart body",
            format!(
                "{:?}",
                RequestBody::from(MultipartBody::new().text("payload_json", JSON_VALUE).file(
                    "file",
                    PATH_TOKEN,
                    "audio/wav",
                    JSON_VALUE
                ))
            ),
        ),
        (
            "api key credential",
            format!(
                "{:?}",
                Credential::ApiKey {
                    header: "x-api-key".to_owned(),
                    key: Secret::new(API_KEY)
                }
            ),
        ),
        (
            "scheme credential",
            format!(
                "{:?}",
                Credential::Scheme {
                    scheme: "OAuth".to_owned(),
                    token: Secret::new(BEARER)
                }
            ),
        ),
        (
            "request with scheme credential",
            format!(
                "{:?}",
                GatewayRequest::get(PATH_TOKEN).authorization("OAuth", Secret::new(BEARER))
            ),
        ),
        (
            "bearer credential",
            format!("{:?}", Credential::Bearer(Secret::new(BEARER))),
        ),
        ("response", format!("{response:?}")),
        ("gateway", format!("{:?}", harness.gateway)),
    ];
    for (what, text) in rendered {
        assert_redacted(&text, what);
    }
}

#[tokio::test]
async fn every_failure_variant_renders_without_url_header_or_credential_text() {
    let echo = format!("echo {BEARER} {QUERY_SECRET}");
    let mut failures = Vec::new();
    for status in [401, 403, 404, 400, 500, 503, 429, 304] {
        let server = serving(ResponseTemplate::new(status).set_body_string(echo.clone())).await;
        failures.push(
            failure_from(
                &server.uri(),
                platform(),
                RecordingRateLimiter::granting(),
                webhook_request(),
            )
            .await,
        );
    }
    let leaky_location = serving(
        ResponseTemplate::new(reqwest::StatusCode::FOUND).insert_header(
            "Location",
            format!("http://192.0.2.1/webhooks/1234/{PATH_TOKEN}?wait={QUERY_SECRET}"),
        ),
    )
    .await;
    failures.push(
        failure_from(
            &leaky_location.uri(),
            platform(),
            RecordingRateLimiter::granting(),
            webhook_request(),
        )
        .await,
    );
    let large = serving(
        ResponseTemplate::new(reqwest::StatusCode::OK).set_body_bytes(vec![
            b'x';
            usize::try_from(
                CallClass::OAuth.body_cap()
            )
            .unwrap()
                + 1
        ]),
    )
    .await;
    failures.push(
        failure_from(
            &large.uri(),
            platform(),
            RecordingRateLimiter::granting(),
            webhook_request().class(CallClass::OAuth),
        )
        .await,
    );
    failures.push(
        failure_from(
            &refused_base(),
            platform(),
            RecordingRateLimiter::granting(),
            webhook_request(),
        )
        .await,
    );
    let garbage = serve_raw_once(b"not http at all\r\n\r\n".to_vec()).await;
    failures.push(
        failure_from(
            &garbage,
            platform(),
            RecordingRateLimiter::granting(),
            webhook_request(),
        )
        .await,
    );
    let truncated = serve_raw_once(
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort".to_vec(),
    )
    .await;
    failures.push(
        failure_from(
            &truncated,
            platform(),
            RecordingRateLimiter::granting(),
            webhook_request(),
        )
        .await,
    );
    let token = CancellationToken::new();
    token.cancel();
    failures.push(
        failure_from(
            &refused_base(),
            platform(),
            RecordingRateLimiter::granting(),
            webhook_request().cancel_on(token),
        )
        .await,
    );
    failures.push(
        failure_from(
            &refused_base(),
            platform(),
            RecordingRateLimiter::exhausted(),
            webhook_request(),
        )
        .await,
    );
    failures.push(
        failure_from(
            &refused_base(),
            platform(),
            RecordingRateLimiter::granting(),
            webhook_request().header("Cookie", HEADER_VALUE),
        )
        .await,
    );
    failures.push(
        Harness::new(
            &format!("https://example.invalid/{BASE_TOKEN}"),
            GatewayProfile::user_directed(false),
        )
        .fail(
            GatewayRequest::absolute(
                Method::Post,
                format!("http://10.0.0.1/webhooks/1/{PATH_TOKEN}?wait={QUERY_SECRET}"),
            )
            .bearer(Secret::new(BEARER)),
        )
        .await,
    );
    let covered: BTreeSet<&str> = failures.iter().map(variant).collect();
    assert_eq!(covered.len(), 13, "variants covered: {covered:?}");
    for failure in &failures {
        assert_redacted(&format!("{failure}"), "failure display");
        assert_redacted(&format!("{failure:?}"), "failure debug");
    }
}

#[tokio::test]
async fn response_decode_error_carries_no_body_text() {
    #[derive(Debug, Deserialize)]
    struct Shape {
        #[expect(dead_code, reason = "held only to keep the channel open")]
        id: u32,
    }
    for (body, category) in [
        (format!(r#"{{"id":"{JSON_VALUE}"}}"#), DecodeCategory::Data),
        (format!("{JSON_VALUE} is not json"), DecodeCategory::Syntax),
        (format!(r#"{{"id":"{JSON_VALUE}"#), DecodeCategory::Eof),
    ] {
        let server =
            serving(ResponseTemplate::new(reqwest::StatusCode::OK).set_body_string(body.clone()))
                .await;
        let response = Harness::new(&server.uri(), platform())
            .ok(GatewayRequest::get("x"))
            .await;
        let error = response.json::<Shape>().unwrap_err();
        assert_eq!(error.category, category, "{body}");
        assert_redacted(&format!("{error}"), "decode display");
        assert_redacted(&format!("{error:?}"), "decode debug");
    }
}
